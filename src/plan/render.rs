//! Human-readable plan, in Terraform's vocabulary.

use super::*;
use crate::format::start_local;
use crate::i18n::{t, tf};

fn describe(cat: &dyn CatalogView, id: &str, code: Option<&str>) -> String {
    match cat.session(id) {
        Some(s) => {
            let place = [start_local(&s, cat.tz()), s.venue.clone().unwrap_or_default()];
            let place: Vec<&str> = place.iter().map(String::as_str).filter(|p| !p.is_empty()).collect();
            format!("{}  {}  ({})", s.code(), s.title, place.join(" "))
        }
        None => code.unwrap_or(id).to_string(),
    }
}

pub fn render_plan(plan: &Plan, cat: &dyn CatalogView) -> String {
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    line(format!("Plan {}  (event {}, expires {})", plan.plan_id, plan.event, plan.expires_at));
    if plan.actions.is_empty() {
        line(t("No changes. Your schedule already matches the spec."));
    }
    let by_seq: std::collections::HashMap<u32, &Action> = plan.actions.iter().map(|a| (a.seq, a)).collect();
    let paired_cancels: std::collections::HashSet<u32> = plan
        .actions
        .iter()
        .filter(|a| a.kind == ActionKind::Reserve)
        .flat_map(|a| a.depends_on.iter().copied())
        .collect();
    let (mut n_res, mut n_can, mut n_rep, mut n_fav, mut n_unf, mut n_blk) = (0, 0, 0, 0, 0, 0);
    for a in &plan.actions {
        let target = a.session_id.as_deref().map(|id| describe(cat, id, a.code.as_deref())).unwrap_or_default();
        match a.kind {
            ActionKind::Cancel if paired_cancels.contains(&a.seq) => {} // shown with its reserve
            ActionKind::Cancel => {
                n_can += 1;
                line(format!(
                    "- cancel  {target}  {}",
                    a.reason.as_deref().map(|r| format!("[{r}]")).unwrap_or_default()
                ));
            }
            ActionKind::Reserve if !a.depends_on.is_empty() => {
                n_rep += 1;
                let old = a
                    .depends_on
                    .first()
                    .and_then(|s| by_seq.get(s))
                    .and_then(|c| c.session_id.as_deref())
                    .map(|id| describe(cat, id, None))
                    .unwrap_or_default();
                line(format!("-/+ replace (seat may be lost)  {old}  ->  {target}"));
            }
            ActionKind::Reserve if a.rereserve => {
                n_res += 1;
                line(format!("! re-reserve (may be full)  {target}"));
            }
            ActionKind::Reserve => {
                n_res += 1;
                line(format!("+ reserve  {target}"));
            }
            ActionKind::Favorite => {
                n_fav += 1;
                line(format!("~ favorite  {target}"));
            }
            ActionKind::Unfavorite => {
                n_unf += 1;
                line(format!("~ unfavorite  {target}"));
            }
            ActionKind::BlockCreate | ActionKind::BlockUpdate => {
                n_blk += 1;
                let verb = if a.kind == ActionKind::BlockCreate { "+ block.create" } else { "~ block.update" };
                line(format!(
                    "{verb}  \"{}\"  {} .. {} UTC",
                    a.title.as_deref().unwrap_or(""),
                    a.start_utc.as_deref().unwrap_or(""),
                    a.end_utc.as_deref().unwrap_or("")
                ));
            }
        }
    }
    for id in &plan.unmanaged {
        line(format!("# unmanaged (untouched)  {}", describe(cat, id, None)));
    }
    for (id, alts) in &plan.alternatives {
        let list: Vec<String> = alts.iter().map(|a| describe(cat, a, None)).collect();
        line(format!("{} {}: {}", t("Alternatives for"), describe(cat, id, None), list.join(" | ")));
    }
    for w in &plan.warnings {
        line(format!("{} {w}", t("warning:")));
    }
    line(tf(
        "Plan: {res} to reserve, {rep} to replace, {can} to cancel, {fav} to favorite, {unf} to unfavorite, {blk} block changes.",
        &[
            ("res", &n_res.to_string()),
            ("rep", &n_rep.to_string()),
            ("can", &n_can.to_string()),
            ("fav", &n_fav.to_string()),
            ("unf", &n_unf.to_string()),
            ("blk", &n_blk.to_string()),
        ],
    ));
    if !plan.required_flags.is_empty() {
        let flags: Vec<String> = plan.required_flags.iter().map(|f| format!("--{f}")).collect();
        line(tf(
            "A replace cancels first; if the new session is full you may lose the seat. Apply requires {flags}.",
            &[("flags", &flags.join(" "))],
        ));
    }
    if !plan.actions.is_empty() {
        let flags: String = plan.required_flags.iter().map(|f| format!(" --{f}")).collect();
        line(tf("To apply: riv apply --plan {id}{flags}", &[("id", &plan.plan_id), ("flags", &flags)]));
    }
    out
}
