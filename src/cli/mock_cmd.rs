use crate::error::{Result, RivError};
use crate::mock::{MOCK_TOKEN, MockServer, Scenario};

pub async fn run(port: u16, scenario: Option<String>) -> Result<i32> {
    let sc = scenario.as_deref().map(Scenario::parse).unwrap_or_default();
    let server =
        MockServer::start(port, sc).await.map_err(|e| RivError::general(format!("cannot bind port {port}: {e}")))?;
    println!("mock Events API (synthetic data) listening on {}", server.base_url);
    println!("events: demo-public (no auth), demo-reinvent (Bearer {MOCK_TOKEN})");
    println!("try:  RIV_API_BASE={} RIV_TOKEN={MOCK_TOKEN} riv sync --event demo-reinvent", server.base_url);
    tokio::signal::ctrl_c().await.map_err(RivError::from)?;
    Ok(0)
}
