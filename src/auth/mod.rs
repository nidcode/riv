//! Authentication: PKCE sign-in, token storage, refresh.

pub mod jwt;
pub mod oauth;
pub mod pkce;
pub mod store;

use crate::error::Result;
use async_trait::async_trait;
pub use store::{Credentials, FileTokenStore, TokenStore};

/// Supplies the bearer token to the API client.
#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn token(&self) -> Option<String>;
    /// Called once after a 401. Ok(Some) = a new token is available.
    async fn refresh(&self) -> Result<Option<String>>;
}

/// No credentials (public events).
pub struct NoToken;

#[async_trait]
impl TokenProvider for NoToken {
    async fn token(&self) -> Option<String> {
        None
    }
    async fn refresh(&self) -> Result<Option<String>> {
        Ok(None)
    }
}

/// `RIV_TOKEN`: skip PKCE (mock server).
pub struct StaticToken(pub String);

#[async_trait]
impl TokenProvider for StaticToken {
    async fn token(&self) -> Option<String> {
        Some(self.0.clone())
    }
    async fn refresh(&self) -> Result<Option<String>> {
        Ok(None)
    }
}

/// Reads and refreshes credentials held in a `TokenStore`.
pub struct StoredTokens<S: TokenStore> {
    store: S,
    oauth_base: String,
    lock: tokio::sync::Mutex<()>,
}

impl<S: TokenStore> StoredTokens<S> {
    pub fn new(store: S) -> Self {
        Self { store, oauth_base: oauth::oauth_base(), lock: tokio::sync::Mutex::new(()) }
    }

    pub fn with_oauth_base(mut self, base: impl Into<String>) -> Self {
        self.oauth_base = base.into();
        self
    }

    async fn do_refresh(&self) -> Result<Option<String>> {
        let _g = self.lock.lock().await;
        let Some(c) = self.store.load()? else { return Ok(None) };
        let Some(rt) = c.refresh_token.clone() else { return Ok(None) };
        let mut fresh = oauth::refresh(&self.oauth_base, &rt).await?;
        if fresh.id_token.is_none() {
            fresh.id_token = c.id_token;
        }
        self.store.save(&fresh)?;
        Ok(Some(fresh.access_token))
    }
}

#[async_trait]
impl<S: TokenStore> TokenProvider for StoredTokens<S> {
    async fn token(&self) -> Option<String> {
        let c = self.store.load().ok().flatten()?;
        if c.expires_at - 60 > chrono::Utc::now().timestamp() {
            return Some(c.access_token);
        }
        self.do_refresh().await.unwrap_or_default()
    }
    async fn refresh(&self) -> Result<Option<String>> {
        self.do_refresh().await
    }
}

/// Choose a provider from the environment: `RIV_TOKEN` wins, then stored credentials.
pub fn provider_from_env() -> std::sync::Arc<dyn TokenProvider> {
    if let Ok(t) = std::env::var("RIV_TOKEN")
        && !t.is_empty()
    {
        return std::sync::Arc::new(StaticToken(t));
    }
    std::sync::Arc::new(StoredTokens::new(FileTokenStore::default_location()))
}
