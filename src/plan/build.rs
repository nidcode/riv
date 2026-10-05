//! The diff itself (pure; no I/O). Semantics are fixed by tests/plan_semantics.rs and brief §7.

use super::*;
use crate::desired::{Desired, DesiredBlock, Want, desired_hash, validate};
use crate::error::{Result, RivError};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

pub struct PlanInput<'a> {
    pub desired: &'a Desired,
    pub schedule: &'a Schedule,
    /// Sessions riv has applied before, with the last `want` (the "managed" set).
    pub managed: &'a HashMap<String, Want>,
    /// block key -> personalTimeId from earlier applies.
    pub block_ids: &'a HashMap<String, String>,
    pub catalog: &'a dyn CatalogView,
    pub now: DateTime<Utc>,
    pub plan_id: String,
    pub catalog_version: String,
    pub account: String,
}

fn rfc(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn code_of(cat: &dyn CatalogView, id: &str) -> String {
    cat.session(id).map(|s| s.code().to_string()).unwrap_or_else(|| id.to_string())
}

fn act(kind: ActionKind, sid: &str, cat: &dyn CatalogView) -> Action {
    Action { kind, session_id: Some(sid.to_string()), code: Some(code_of(cat, sid)), ..Default::default() }
}

/// Validate against the catalog, then build the plan. Validation errors reject the plan (exit code 2).
pub fn make_plan(input: &PlanInput<'_>) -> Result<Plan> {
    let errs = validate(input.desired, &|id| input.catalog.session(id).is_some());
    if !errs.is_empty() {
        return Err(RivError::validation(errs.join("\n")));
    }
    Ok(build_plan(input))
}

pub fn build_plan(i: &PlanInput<'_>) -> Plan {
    let cat = i.catalog;
    let obs_res: HashSet<&String> = i.schedule.reserved.iter().collect();
    let obs_fav: HashSet<&String> = i.schedule.favorites.iter().collect();
    let desired_ids: HashSet<&String> = i.desired.sessions.iter().map(|s| &s.id).collect();

    let mut warnings: Vec<String> = Vec::new();
    let mut cancels: Vec<Action> = Vec::new();
    let mut unfavs: Vec<Action> = Vec::new();
    let mut pairs: Vec<(Action, Action)> = Vec::new();
    let mut reserves: Vec<Action> = Vec::new();
    let mut favs: Vec<Action> = Vec::new();
    let mut warned_unmanaged: Vec<String> = Vec::new();
    let mut cancelled_by_replace: HashSet<String> = HashSet::new();

    for ds in i.desired.sessions.iter().filter(|s| !s.pin) {
        let me = code_of(cat, &ds.id);
        let managed_as = i.managed.get(&ds.id).copied();
        match ds.want {
            Want::Reserved => {
                let replaced = ds.replaces.as_ref().filter(|r| obs_res.contains(r));
                if obs_res.contains(&ds.id) {
                    // Target already held: dropping the old seat risks nothing.
                    if let Some(x) = replaced {
                        let mut c = act(ActionKind::Cancel, x, cat);
                        c.reason = Some(format!("replaced by {me}"));
                        cancelled_by_replace.insert(x.clone());
                        cancels.push(c);
                    }
                    continue;
                }
                let mut r = act(ActionKind::Reserve, &ds.id, cat);
                r.rereserve = managed_as == Some(Want::Reserved);
                if r.rereserve {
                    r.reason = Some("reservation no longer observed (cancelled elsewhere?); may be full".into());
                }
                match replaced {
                    Some(x) => {
                        let mut c = act(ActionKind::Cancel, x, cat);
                        c.reason = Some(format!("replaced by {me}"));
                        c.risk = Some("seat-loss".into());
                        cancelled_by_replace.insert(x.clone());
                        pairs.push((c, r));
                    }
                    None => reserves.push(r),
                }
            }
            Want::Favorite => {
                if ds.replaces.is_some() {
                    warnings.push(format!("{me}: `replaces` only applies to want: reserved; ignored"));
                }
                if obs_res.contains(&ds.id) {
                    warnings.push(format!("{me}: already reserved; want: favorite leaves it as is"));
                } else if !obs_fav.contains(&ds.id) {
                    favs.push(act(ActionKind::Favorite, &ds.id, cat));
                }
            }
            Want::None => {
                if obs_res.contains(&ds.id) {
                    if managed_as == Some(Want::Reserved) {
                        let mut c = act(ActionKind::Cancel, &ds.id, cat);
                        c.reason = Some("want: none".into());
                        cancels.push(c);
                    } else {
                        warnings.push(format!(
                            "{me}: want: none, but this reservation is not managed by riv; left untouched"
                        ));
                        warned_unmanaged.push(ds.id.clone());
                    }
                }
                if obs_fav.contains(&ds.id) {
                    if managed_as == Some(Want::Favorite) {
                        let mut u = act(ActionKind::Unfavorite, &ds.id, cat);
                        u.reason = Some("want: none".into());
                        unfavs.push(u);
                    } else {
                        warnings
                            .push(format!("{me}: want: none, but this favorite is not managed by riv; left untouched"));
                        warned_unmanaged.push(ds.id.clone());
                    }
                }
            }
        }
    }

    let (blocks, block_warnings) = plan_blocks(i);
    warnings.extend(block_warnings);

    // Execution order: standalone cancels, unfavorites, replace pairs (cancel immediately followed by its reserve),
    // reserves, favorites, blocks.
    let mut ordered: Vec<Action> = Vec::new();
    ordered.extend(cancels);
    ordered.extend(unfavs);
    for (c, r) in pairs {
        let c_seq = (ordered.len() + 1) as u32;
        ordered.push(c);
        let mut r = r;
        r.depends_on = vec![c_seq];
        ordered.push(r);
    }
    ordered.extend(reserves);
    ordered.extend(favs);
    ordered.extend(blocks);
    for (n, a) in ordered.iter_mut().enumerate() {
        a.seq = (n + 1) as u32;
    }
    // Pair dependencies were computed before renumbering ended up equal, because seq == index + 1.

    let mut alternatives: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for a in ordered.iter().filter(|a| a.kind == ActionKind::Reserve) {
        let Some(sid) = &a.session_id else { continue };
        let Some(s) = cat.session(sid) else { continue };
        if s.is_reservable == Some(false) {
            warnings.push(format!(
                "{}: not reservable (walk-up or restricted); the server will refuse it",
                code_of(cat, sid)
            ));
        }
        if s.seat_availability.is_some_and(|v| v.is_full()) {
            let alts: Vec<String> = cat
                .same_title(&s)
                .into_iter()
                .filter(|o| {
                    o.session_id != *sid
                        && o.is_reservable != Some(false)
                        && !o.seat_availability.is_some_and(|v| v.is_full())
                })
                .map(|o| o.session_id)
                .collect();
            warnings.push(format!("{}: appears full; consider an alternative run", code_of(cat, sid)));
            if !alts.is_empty() {
                alternatives.insert(sid.clone(), alts);
            }
        }
    }
    warnings.extend(overlap_warnings(i, &ordered, &cancelled_by_replace));

    let mut unmanaged: Vec<String> = i
        .schedule
        .reserved
        .iter()
        .chain(i.schedule.favorites.iter())
        .filter(|id| !desired_ids.contains(id))
        .cloned()
        .chain(warned_unmanaged)
        .filter(|id| !cancelled_by_replace.contains(id))
        .collect();
    unmanaged.sort();
    unmanaged.dedup();

    let required_flags = if ordered.iter().any(|a| a.risk.as_deref() == Some("seat-loss")) {
        vec![FLAG_SEAT_LOSS.to_string()]
    } else {
        vec![]
    };

    Plan {
        schema_version: SCHEMA_VERSION,
        plan_id: i.plan_id.clone(),
        created_at: rfc(i.now),
        expires_at: rfc(i.now + chrono::Duration::minutes(PLAN_TTL_MINUTES)),
        event: i.desired.event.clone(),
        account: i.account.clone(),
        desired_hash: desired_hash(i.desired),
        observed_hash: observed_hash(i.schedule),
        catalog_version: i.catalog_version.clone(),
        actions: ordered,
        unmanaged,
        alternatives,
        warnings,
        required_flags,
    }
}

fn same_block(b: &DesiredBlock, pt: &crate::api::PersonalTime, range: &(String, String)) -> bool {
    pt.title == b.title
        && pt.description == b.description
        && pt.location.as_deref().unwrap_or("") == b.location.as_deref().unwrap_or("")
        && pt.start_date_time == range.0
        && pt.end_date_time == range.1
}

fn plan_blocks(i: &PlanInput<'_>) -> (Vec<Action>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for b in &i.desired.blocks {
        let Some(range) = b.utc_range(i.catalog.tz()) else { continue };
        let existing =
            i.block_ids.get(&b.key).and_then(|id| i.schedule.personal_time.iter().find(|p| &p.personal_time_id == id));
        let mut a = Action {
            key: Some(b.key.clone()),
            title: Some(b.title.clone()),
            description: Some(b.description.clone()),
            location: b.location.clone(),
            start_utc: Some(range.0.clone()),
            end_utc: Some(range.1.clone()),
            ..Default::default()
        };
        match existing {
            Some(pt) if same_block(b, pt, &range) => continue,
            Some(pt) => {
                a.kind = ActionKind::BlockUpdate;
                a.personal_time_id = Some(pt.personal_time_id.clone());
                out.push(a);
            }
            None => {
                if let Some(pt) = i
                    .schedule
                    .personal_time
                    .iter()
                    .find(|p| p.title == b.title && p.start_date_time == range.0 && p.end_date_time == range.1)
                {
                    warnings.push(format!(
                        "block `{}`: an identical personal time already exists ({}); not creating a duplicate",
                        b.key, pt.personal_time_id
                    ));
                    continue;
                }
                a.kind = ActionKind::BlockCreate;
                out.push(a);
            }
        }
    }
    (out, warnings)
}

/// Client-side clash hints for newly reserved sessions (the server stays the judge).
fn overlap_warnings(i: &PlanInput<'_>, actions: &[Action], cancelled: &HashSet<String>) -> Vec<String> {
    let tz = i.catalog.tz();
    let range = |id: &str| i.catalog.session(id).and_then(|s| s.range_utc(tz));
    let mut final_res: Vec<String> = i
        .schedule
        .reserved
        .iter()
        .filter(|r| {
            !cancelled.contains(*r)
                && !actions.iter().any(|a| a.kind == ActionKind::Cancel && a.session_id.as_deref() == Some(r.as_str()))
        })
        .cloned()
        .collect();
    let new: Vec<String> =
        actions.iter().filter(|a| a.kind == ActionKind::Reserve).filter_map(|a| a.session_id.clone()).collect();
    final_res.extend(new.iter().cloned());
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for n in &new {
        let Some((s1, e1)) = range(n) else { continue };
        for other in final_res.iter().filter(|o| *o != n) {
            let Some((s2, e2)) = range(other) else { continue };
            let key = if n < other { (n.clone(), other.clone()) } else { (other.clone(), n.clone()) };
            if s1 < e2 && s2 < e1 && seen.insert(key) {
                out.push(format!("{} overlaps {}", code_of(i.catalog, n), code_of(i.catalog, other)));
            }
        }
        for pt in &i.schedule.personal_time {
            if let (Some(a), Some(b)) =
                (crate::timeutil::from_wire(&pt.start_date_time), crate::timeutil::from_wire(&pt.end_date_time))
                && s1 < b
                && a < e1
            {
                out.push(format!("{} overlaps personal time \"{}\"", code_of(i.catalog, n), pt.title));
            }
        }
    }
    out
}
