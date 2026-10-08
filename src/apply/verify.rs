//! Compare desired state with an observed schedule. Pure; used by `apply` (final check) and `riv verify`.

use crate::api::Schedule;
use crate::desired::{Desired, Want};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerifyItem {
    pub target: String,
    pub expected: String,
    pub observed: String,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct VerifyReport {
    pub items: Vec<VerifyItem>,
    pub pinned: Vec<String>,
}

impl VerifyReport {
    pub fn all_ok(&self) -> bool {
        self.items.iter().all(|i| i.ok)
    }
    pub fn diffs(&self) -> impl Iterator<Item = &VerifyItem> {
        self.items.iter().filter(|i| !i.ok)
    }
}

fn state_of(s: &Schedule, id: &str) -> &'static str {
    if s.reserved.iter().any(|r| r == id) {
        "reserved"
    } else if s.favorites.iter().any(|f| f == id) {
        "favorite"
    } else {
        "none"
    }
}

pub fn verify(d: &Desired, s: &Schedule, block_ids: &HashMap<String, String>, tz: chrono_tz::Tz) -> VerifyReport {
    let mut rep = VerifyReport::default();
    for ds in &d.sessions {
        if ds.pin {
            rep.pinned.push(ds.id.clone());
            continue;
        }
        let observed = state_of(s, &ds.id);
        let ok = match ds.want {
            Want::Reserved => observed == "reserved",
            // a reservation also satisfies interest in a session
            Want::Favorite => observed == "favorite" || observed == "reserved",
            Want::None => observed == "none",
        };
        rep.items.push(VerifyItem {
            target: ds.id.clone(),
            expected: ds.want.as_str().into(),
            observed: observed.into(),
            ok,
        });
    }
    for b in &d.blocks {
        if b.want == crate::desired::BlockWant::None {
            // Only a block riv created (remembered id) counts; a look-alike made by hand is not ours.
            let still_there =
                block_ids.get(&b.key).is_some_and(|id| s.personal_time.iter().any(|p| &p.personal_time_id == id));
            rep.items.push(VerifyItem {
                target: format!("block:{}", b.key),
                expected: "absent".into(),
                observed: if still_there { "present" } else { "absent" }.into(),
                ok: !still_there,
            });
            continue;
        }
        let range = b.utc_range(tz);
        let present = range.is_some_and(|(st, en)| {
            s.personal_time.iter().any(|p| {
                block_ids.get(&b.key).is_some_and(|id| *id == p.personal_time_id)
                    || (p.title == b.title && p.start_date_time == st && p.end_date_time == en)
            })
        });
        rep.items.push(VerifyItem {
            target: format!("block:{}", b.key),
            expected: "present".into(),
            observed: if present { "present" } else { "absent" }.into(),
            ok: present,
        });
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desired::parse;

    #[test]
    fn reports_diffs() {
        let d = parse("# riv:desired-state v1\nevent: e\ntimezone: America/Los_Angeles\nsessions:\n  - {id: a, want: reserved}\n  - {id: b, want: none}\n  - {id: c, want: favorite, pin: true}\n").expect("parse");
        let s = Schedule { reserved: vec!["b".into()], ..Default::default() };
        let r = verify(&d, &s, &HashMap::new(), chrono_tz::UTC);
        assert_eq!(r.diffs().count(), 2);
        assert_eq!(r.pinned, vec!["c".to_string()]);
        assert!(!r.all_ok());
    }
}
