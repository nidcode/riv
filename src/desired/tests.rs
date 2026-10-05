use super::*;

const OK: &str = "# riv:desired-state v1
event: reinvent2026
timezone: America/Los_Angeles
sessions:
  - id: \"a\"
    code: \"AIM301\"
    want: reserved
    note: why
  - id: \"b\"
    want: favorite
    pin: true
    replaces: \"a\"
blocks:
  - key: meetup
    title: Meetup
    description: Dinner
    start: \"2026-12-01T19:00\"
    end: \"2026-12-01T21:00\"
";

fn doc(yaml: &str) -> String {
    format!("# Design\n\ntext\n\n```yaml\n{yaml}\n```\n")
}

fn known(_: &str) -> bool {
    true
}

#[test]
fn parses_and_extracts() {
    let d = parse_markdown(&doc(OK)).expect("parse");
    assert_eq!(d.sessions.len(), 2);
    assert_eq!(d.sessions[0].want, Want::Reserved);
    assert!(d.sessions[1].pin);
    assert_eq!(
        d.blocks[0].utc_range(chrono_tz::America::Los_Angeles),
        Some(("2026-12-02T03:00:00".into(), "2026-12-02T05:00:00".into()))
    );
}

#[test]
fn needs_exactly_one_block_with_header() {
    assert!(parse_markdown("# no yaml").is_err());
    let two = format!("{}\n{}", doc(OK), doc(OK));
    assert!(parse_markdown(&two).is_err());
    assert!(parse("event: x\n").is_err(), "header is mandatory");
    // an unrelated yaml block is ignored
    let md = format!("```yaml\nfoo: 1\n```\n{}", doc(OK));
    assert!(parse_markdown(&md).is_ok());
}

fn errs_of(yaml: &str, known: &dyn Fn(&str) -> bool) -> Vec<String> {
    match parse(yaml) {
        Ok(d) => validate(&d, known),
        Err(e) => vec![e.message],
    }
}

fn base(sessions: &str, blocks: &str) -> String {
    format!("{HEADER}\nevent: e\ntimezone: America/Los_Angeles\nsessions:\n{sessions}\nblocks:\n{blocks}\n")
}

#[test]
fn validation_table() {
    let cases: Vec<(&str, String, &str)> = vec![
        ("unknown id", base("  - {id: zz, want: reserved}", "  []"), "unknown sessionId `zz`"),
        ("bad want", base("  - {id: a, want: maybe}", "  []"), "invalid want `maybe`"),
        (
            "duplicate id",
            base("  - {id: a, want: reserved}\n  - {id: a, want: favorite}", "  []"),
            "duplicate session id `a`",
        ),
        ("self replace", base("  - {id: a, want: reserved, replaces: a}", "  []"), "replaces itself"),
        (
            "not 5 minutes",
            base(
                "  []",
                "  - {key: k, title: t, description: d, start: \"2026-12-01T19:00\", end: \"2026-12-01T19:07\"}",
            ),
            "multiple of 5",
        ),
        (
            "end before start",
            base(
                "  []",
                "  - {key: k, title: t, description: d, start: \"2026-12-01T19:00\", end: \"2026-12-01T18:00\"}",
            ),
            "after start",
        ),
        (
            "missing description is a parse error",
            base("  []", "  - {key: k, title: t, start: \"2026-12-01T19:00\", end: \"2026-12-01T20:00\"}"),
            "description",
        ),
        (
            "duplicate block key",
            base(
                "  []",
                "  - {key: k, title: t, description: d, start: \"2026-12-01T19:00\", end: \"2026-12-01T20:00\"}\n  - {key: k, title: t, description: d, start: \"2026-12-01T21:00\", end: \"2026-12-01T22:00\"}",
            ),
            "duplicate block key",
        ),
    ];
    for (name, yaml, needle) in cases {
        let known = |id: &str| id != "zz";
        let errs = errs_of(&yaml, &known);
        assert!(errs.iter().any(|e| e.contains(needle)), "{name}: {errs:?}");
    }
    assert!(errs_of(&base("  - {id: a, want: reserved}", "  []"), &known).is_empty());
}

#[test]
fn hash_ignores_note_code_and_order_but_not_meaning() {
    let a = parse(&base("  - {id: a, want: reserved, note: x}\n  - {id: b, want: favorite}", "  []")).expect("a");
    let b =
        parse(&base("  - {id: b, want: favorite, code: C}\n  - {id: a, want: reserved, note: y}", "  []")).expect("b");
    let c = parse(&base("  - {id: a, want: favorite}\n  - {id: b, want: favorite}", "  []")).expect("c");
    assert_eq!(desired_hash(&a), desired_hash(&b));
    assert_ne!(desired_hash(&a), desired_hash(&c));
}
