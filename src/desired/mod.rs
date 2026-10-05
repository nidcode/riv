//! Desired state: one YAML block at the end of design.md, parsed and validated.

use crate::error::{Result, RivError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const HEADER: &str = "# riv:desired-state v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Want {
    Reserved,
    Favorite,
    None,
}

impl Want {
    pub fn as_str(self) -> &'static str {
        match self {
            Want::Reserved => "reserved",
            Want::Favorite => "favorite",
            Want::None => "none",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "reserved" => Some(Want::Reserved),
            "favorite" => Some(Want::Favorite),
            "none" => Some(Want::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSession {
    id: String,
    code: Option<String>,
    want: String,
    #[serde(default)]
    pin: bool,
    replaces: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBlock {
    key: String,
    title: String,
    description: String,
    start: String,
    end: String,
    location: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDesired {
    event: String,
    timezone: String,
    #[serde(default)]
    sessions: Vec<RawSession>,
    #[serde(default)]
    blocks: Vec<RawBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredSession {
    pub id: String,
    pub code: Option<String>,
    pub want: Want,
    pub pin: bool,
    pub replaces: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DesiredBlock {
    pub key: String,
    pub title: String,
    pub description: String,
    /// Local time in `timezone`, `YYYY-MM-DDTHH:MM`.
    pub start: String,
    pub end: String,
    pub location: Option<String>,
}

impl DesiredBlock {
    /// UTC wire-format range (no offset, seconds 00) or None if the local time is invalid.
    pub fn utc_range(&self, tz: chrono_tz::Tz) -> Option<(String, String)> {
        let s = crate::timeutil::local_to_utc(crate::timeutil::parse_local(&self.start)?, tz)?;
        let e = crate::timeutil::local_to_utc(crate::timeutil::parse_local(&self.end)?, tz)?;
        Some((crate::timeutil::to_wire(s), crate::timeutil::to_wire(e)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Desired {
    pub event: String,
    pub timezone: String,
    pub sessions: Vec<DesiredSession>,
    pub blocks: Vec<DesiredBlock>,
}

/// Pull the single `# riv:desired-state v1` YAML code block out of a Markdown document.
pub fn extract_yaml(markdown: &str) -> Result<String> {
    let mut blocks = Vec::new();
    let mut cur: Option<Vec<&str>> = None;
    for line in markdown.lines() {
        let t = line.trim_start();
        match (&mut cur, t.starts_with("```")) {
            (None, true) if t.trim_start_matches('`').trim().starts_with("yaml") => cur = Some(Vec::new()),
            (Some(buf), true) => {
                blocks.push(buf.join("\n"));
                cur = None;
            }
            (Some(buf), false) => buf.push(line),
            _ => {}
        }
    }
    let mut found: Vec<String> = blocks.into_iter().filter(|b| b.trim_start().starts_with(HEADER)).collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(RivError::validation(format!("no YAML block starting with `{HEADER}` found in the spec"))),
        n => Err(RivError::validation(format!("found {n} desired-state blocks; exactly one is allowed"))),
    }
}

pub fn parse(yaml: &str) -> Result<Desired> {
    if !yaml.trim_start().starts_with(HEADER) {
        return Err(RivError::validation(format!("the first line must be `{HEADER}`")));
    }
    let raw: RawDesired =
        serde_yaml_ng::from_str(yaml).map_err(|e| RivError::validation(format!("desired state YAML: {e}")))?;
    let mut errors = Vec::new();
    let mut sessions = Vec::new();
    for s in raw.sessions {
        match Want::parse(&s.want) {
            Some(want) => sessions.push(DesiredSession {
                id: s.id,
                code: s.code,
                want,
                pin: s.pin,
                replaces: s.replaces,
                note: s.note,
            }),
            None => errors.push(format!("session {}: invalid want `{}` (reserved|favorite|none)", s.id, s.want)),
        }
    }
    if !errors.is_empty() {
        return Err(RivError::validation(errors.join("\n")));
    }
    let blocks = raw
        .blocks
        .into_iter()
        .map(|b| DesiredBlock {
            key: b.key,
            title: b.title,
            description: b.description,
            start: b.start,
            end: b.end,
            location: b.location,
        })
        .collect();
    Ok(Desired { event: raw.event, timezone: raw.timezone, sessions, blocks })
}

pub fn parse_markdown(md: &str) -> Result<Desired> {
    parse(&extract_yaml(md)?)
}

/// All semantic problems with a desired state. Empty = valid. `known` says whether a sessionId exists in the catalog.
pub fn validate(d: &Desired, known: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut errs = Vec::new();
    let Some(tz) = crate::timeutil::parse_tz(&d.timezone) else {
        errs.push(format!("unknown timezone `{}`", d.timezone));
        return errs;
    };
    let mut seen = HashSet::new();
    for s in &d.sessions {
        if !seen.insert(&s.id) {
            errs.push(format!("duplicate session id `{}`", s.id));
        }
        if !known(&s.id) {
            errs.push(format!("unknown sessionId `{}` (use the API sessionId, not the code; run `riv sync`)", s.id));
        }
        if let Some(r) = &s.replaces {
            if r == &s.id {
                errs.push(format!("session `{}` replaces itself", s.id));
            } else if !known(r) {
                errs.push(format!("session `{}`: replaces unknown sessionId `{r}`", s.id));
            }
        }
    }
    let mut keys = HashSet::new();
    for b in &d.blocks {
        if !keys.insert(&b.key) {
            errs.push(format!("duplicate block key `{}`", b.key));
        }
        errs.extend(validate_block(b, tz));
    }
    errs
}

fn validate_block(b: &DesiredBlock, tz: chrono_tz::Tz) -> Vec<String> {
    let mut errs = Vec::new();
    let k = &b.key;
    let range = match (crate::timeutil::parse_local(&b.start), crate::timeutil::parse_local(&b.end)) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => {
            errs.push(format!("block `{k}`: start/end must be YYYY-MM-DDTHH:MM"));
            None
        }
    };
    if let Some((s, e)) = range {
        let mins = (e - s).num_minutes();
        if mins <= 0 {
            errs.push(format!("block `{k}`: end must be after start"));
        } else if mins % 5 != 0 {
            errs.push(format!("block `{k}`: length must be a multiple of 5 minutes"));
        }
        if crate::timeutil::local_to_utc(s, tz).is_none() || crate::timeutil::local_to_utc(e, tz).is_none() {
            errs.push(format!("block `{k}`: start/end do not exist in {tz} (DST gap)"));
        }
        if s.and_utc().timestamp() % 60 != 0 {
            errs.push(format!("block `{k}`: seconds must be 00"));
        }
    }
    let len = |s: &str| s.chars().count();
    if b.title.is_empty() || len(&b.title) > 128 {
        errs.push(format!("block `{k}`: title must be 1-128 characters"));
    }
    if b.description.is_empty() || len(&b.description) > 250 {
        errs.push(format!("block `{k}`: description must be 1-250 characters"));
    }
    if b.location.as_deref().is_some_and(|l| l.is_empty() || len(l) > 255) {
        errs.push(format!("block `{k}`: location must be 1-255 characters when given"));
    }
    errs
}

/// Hash of the *meaning* of the desired state. Display-only fields (code, note) and ordering do not count.
pub fn desired_hash(d: &Desired) -> String {
    let mut sessions: Vec<_> = d.sessions.iter().map(|s| (&s.id, s.want.as_str(), s.pin, &s.replaces)).collect();
    sessions.sort();
    let mut blocks: Vec<_> = d.blocks.iter().collect();
    blocks.sort_by(|a, b| a.key.cmp(&b.key));
    let canon = serde_json::json!({ "event": d.event, "timezone": d.timezone, "sessions": sessions, "blocks": blocks });
    format!("sha256:{}", hex::encode(Sha256::digest(canon.to_string().as_bytes())))
}

#[cfg(test)]
mod tests;
