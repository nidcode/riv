use super::ctx::*;
use crate::error::{Result, RivError};
use crate::plan::service::create_plan;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

pub const DEFAULT_SPEC: &str = ".kiro/specs/reinvent-2026/design.md";

pub async fn plan(spec: Option<PathBuf>, json: bool) -> Result<i32> {
    let spec = spec.unwrap_or_else(|| PathBuf::from(DEFAULT_SPEC));
    let db = open_db()?;
    let out = create_plan(&make_api(), &db, &spec, &crate::paths::plans_dir(), &crate::auth::account_id()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&out.plan)?);
    } else {
        print!("{}", out.rendered);
        eprintln!("saved: {}", out.path.display());
    }
    Ok(0)
}

fn ask(question: &str, interactive: bool) -> String {
    if !interactive {
        return "TBD".into();
    }
    print!("{question} ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    let line = line.trim();
    if line.is_empty() { "TBD".into() } else { line.to_string() }
}

pub fn init(dir: PathBuf, goal: Option<String>, interests: Option<String>, constraints: Option<String>) -> Result<i32> {
    let target = dir.join(".kiro/specs/reinvent-2026");
    std::fs::create_dir_all(&target)?;
    let interactive =
        std::io::stdin().is_terminal() && (goal.is_none() || interests.is_none() || constraints.is_none());
    let goal = goal.unwrap_or_else(|| ask("1/3 What do you want from re:Invent this year (purpose)?", interactive));
    let interests = interests.unwrap_or_else(|| ask("2/3 Which topics/services interest you?", interactive));
    let constraints =
        constraints.unwrap_or_else(|| ask("3/3 Constraints (days off, travel time, must-attend events)?", interactive));
    let requirements = crate::templates::REQUIREMENTS
        .replace("{{goal}}", &goal)
        .replace("{{interests}}", &interests)
        .replace("{{constraints}}", &constraints);
    for (name, body) in [
        ("requirements.md", requirements.as_str()),
        ("design.md", crate::templates::DESIGN),
        ("tasks.md", crate::templates::TASKS),
    ] {
        let p = target.join(name);
        if p.exists() {
            eprintln!("kept existing {}", p.display());
            continue;
        }
        std::fs::write(&p, body).map_err(RivError::from)?;
        println!("created {}", p.display());
    }
    Ok(0)
}
