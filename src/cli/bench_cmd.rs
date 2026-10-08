use super::ctx::*;
use crate::bench::{self, BenchEnv};
use crate::error::{Result, RivError};
use std::path::PathBuf;

pub async fn run(
    name: &str,
    event: String,
    mock: bool,
    runs: usize,
    fixture: Option<PathBuf>,
    generate: Option<usize>,
    out: PathBuf,
) -> Result<i32> {
    let env = if mock {
        let mut e = BenchEnv::mock(runs, fixture).await?;
        e.generate = generate;
        e
    } else {
        BenchEnv {
            api_base: api_base(),
            tokens: crate::auth::provider_from_env(),
            event,
            mock: false,
            runs,
            fixture,
            generate,
            _mock_server: None,
        }
    };
    let report = match name {
        "protocol" => bench::protocol(&env).await?,
        "first-run" => bench::first_run(&env).await?,
        "warm" => bench::warm(&env).await?,
        "search-quality" => bench::search_quality(&env).await?,
        other => {
            return Err(RivError::validation(format!(
                "unknown bench `{other}` (protocol|first-run|warm|search-quality)"
            )));
        }
    };
    println!("{}\n{}", report.procedure, report.table);
    println!("written to {}", bench::write_report(&out, &env, &report)?.display());
    Ok(0)
}
