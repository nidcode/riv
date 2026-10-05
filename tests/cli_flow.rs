//! End-to-end CLI run (the 30-second quick start) against the in-process mock.

use assert_cmd::Command;
use riv::mock::{MOCK_TOKEN, MockServer, Scenario};

struct Cli {
    base: String,
    data: tempfile::TempDir,
    work: tempfile::TempDir,
}

impl Cli {
    fn run(&self, args: &[&str], lang: &str) -> (i32, String, String) {
        let out = Command::cargo_bin("riv")
            .expect("bin")
            .current_dir(self.work.path())
            .env("RIV_API_BASE", &self.base)
            .env("RIV_TOKEN", MOCK_TOKEN)
            .env("RIV_DATA_DIR", self.data.path().join("data"))
            .env("RIV_CONFIG_DIR", self.data.path().join("config"))
            .env("RIV_LANG", lang)
            .args(args)
            .output()
            .expect("run");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into(),
            String::from_utf8_lossy(&out.stderr).into(),
        )
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn quick_start_flow() {
    let server = MockServer::start(0, Scenario::default()).await.expect("mock");
    let cli = Cli {
        base: server.base_url.clone(),
        data: tempfile::tempdir().expect("d"),
        work: tempfile::tempdir().expect("w"),
    };
    let c = std::sync::Arc::new(cli);
    let c2 = c.clone();
    let result = tokio::task::spawn_blocking(move || {
        let c = c2;
        let ev = "demo-reinvent";
        let (code, out, _) = c.run(&["events"], "en");
        assert_eq!(code, 0);
        assert!(out.contains("demo-public") && out.contains("sign-in required"));
        let (code, out, _) = c.run(&["sync", "--event", ev], "en");
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("Synced 120 sessions"));
        let (_, out, _) = c.run(&["search", "--event", ev, "--limit", "1", "--fields", "id", "bedrock"], "en");
        let id = out.trim().to_string();
        assert!(id.starts_with("mock-"), "{out}");
        let (_, out, _) = c.run(&["search", "--event", ev, "zzzqqq"], "ja");
        assert!(out.contains("該当なし"), "{out}");
        let (_, out, _) = c.run(&["search", "--event", ev, "--limit", "2", "--json", "bedrock"], "en");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out).expect("json").as_array().map(Vec::len), Some(2));
        assert_eq!(c.run(&["show", "--event", ev, "nope"], "en").0, 1);

        assert_eq!(c.run(&["init", ".", "--goal", "g", "--interests", "i", "--constraints", "c"], "en").0, 0);
        let spec = c.work.path().join(".kiro/specs/reinvent-2026/design.md");
        let text = std::fs::read_to_string(&spec).expect("design.md");
        std::fs::write(
            &spec,
            text.replace("event: reinvent2026", &format!("event: {ev}"))
                .replace("sessions: []", &format!("sessions:\n  - {{id: {id}, want: favorite}}")),
        )
        .expect("write");
        let (code, out, _) = c.run(&["plan", "--json"], "en");
        assert_eq!(code, 0, "{out}");
        let plan: serde_json::Value = serde_json::from_str(&out).expect("plan json");
        let pid = plan["planId"].as_str().expect("planId").to_string();
        assert_eq!(plan["actions"][0]["kind"], "favorite");
        // apply without a plan id is refused (exit 4)
        assert_eq!(c.run(&["apply", "--yes"], "en").0, 4);
        let (code, out, _) = c.run(&["apply", "--plan", &pid, "--yes"], "en");
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("DONE"));
        assert_eq!(c.run(&["verify"], "en").0, 0);
        let (_, out, _) = c.run(&["schedule", "--event", ev], "ja");
        assert!(out.contains("お気に入り (1)"), "{out}");
        // prep + today
        let (code, out, _) = c.run(&["prep", &id, "--event", ev, "--json"], "ja");
        assert_eq!(code, 0, "{out}");
        let pack: serde_json::Value = serde_json::from_str(&out).expect("pack json");
        let save = pack["savePath"].as_str().expect("savePath").to_string();
        let note = c.work.path().join(&save);
        std::fs::create_dir_all(note.parent().expect("dir")).expect("mkdir");
        std::fs::write(&note, "# note").expect("note");
        assert_eq!(c.run(&["prep", "--attach", &id, note.to_str().expect("path"), "--event", ev], "en").0, 0);
        let (code, out, _) = c.run(&["today", "--event", ev, "--date", "2026-12-01", "--format", "phone"], "ja");
        assert_eq!(code, 0, "{out}");
        assert!(out.contains("予定はありません"), "{out}");
        assert_eq!(c.run(&["today", "--event", ev, "--date", "nope"], "en").0, 2);
        // a second plan has nothing to do
        let (_, out, _) = c.run(&["plan"], "en");
        assert!(out.contains("No changes"), "{out}");
        // validation error: exit 2
        std::fs::write(
            &spec,
            text.replace("event: reinvent2026", &format!("event: {ev}"))
                .replace("sessions: []", "sessions:\n  - {id: bogus, want: reserved}"),
        )
        .expect("write");
        assert_eq!(c.run(&["plan"], "en").0, 2);
        // tasks.md was generated
        assert!(
            std::fs::read_to_string(c.work.path().join(".kiro/specs/reinvent-2026/tasks.md"))
                .expect("tasks")
                .contains("generated by riv")
        );
    })
    .await;
    result.expect("cli flow");
}
