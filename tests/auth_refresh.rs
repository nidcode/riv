use riv::api::EventsApi;
use riv::api::http::HttpApi;
use riv::auth::{Credentials, FileTokenStore, StoredTokens, TokenStore};
use riv::mock::{MockServer, Scenario};
use std::sync::Arc;

#[tokio::test]
async fn expired_access_token_is_refreshed_and_rotated_refresh_token_saved() {
    let s = MockServer::start(0, Scenario::default()).await.expect("mock");
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("credentials.json");
    FileTokenStore::new(path.clone())
        .save(&Credentials {
            access_token: "stale".into(),
            refresh_token: Some("mock-refresh".into()),
            id_token: None,
            expires_at: 0,
        })
        .expect("save");
    let tokens = StoredTokens::new(FileTokenStore::new(path.clone())).with_oauth_base(s.base_url.clone());
    let api = HttpApi::new(s.base_url.clone(), Arc::new(tokens)).with_sleep_scale(0.0);
    api.get_schedule("demo-reinvent").await.expect("refreshed transparently");
    let saved = FileTokenStore::new(path).load().expect("load").expect("some");
    assert_eq!(saved.access_token, "mock-token");
    assert_eq!(saved.refresh_token.as_deref(), Some("mock-refresh-2"));
}

#[tokio::test]
async fn bad_refresh_token_yields_unauthorized_not_a_loop() {
    let s = MockServer::start(0, Scenario::default()).await.expect("mock");
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("credentials.json");
    FileTokenStore::new(path.clone())
        .save(&Credentials {
            access_token: "stale".into(),
            refresh_token: Some("nope".into()),
            id_token: None,
            expires_at: 0,
        })
        .expect("save");
    let tokens = StoredTokens::new(FileTokenStore::new(path)).with_oauth_base(s.base_url.clone());
    let api = HttpApi::new(s.base_url.clone(), Arc::new(tokens)).with_sleep_scale(0.0);
    assert_eq!(api.get_schedule("demo-reinvent").await.unwrap_err(), riv::api::ApiError::Unauthorized);
}
