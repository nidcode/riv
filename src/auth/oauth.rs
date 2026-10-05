//! OAuth 2.0 authorization code + PKCE against oauth.awsevents.com (base overridable for tests).

use super::pkce::Pkce;
use super::store::Credentials;
use crate::error::{Result, RivError};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CLIENT_ID: &str = "7vmom55m1qstvq8i71ph127bfq";
pub const SCOPE: &str = "openid email events/access";
pub const PORTS: std::ops::RangeInclusive<u16> = 8484..=8489;
pub const DEFAULT_OAUTH_BASE: &str = "https://oauth.awsevents.com";

pub fn oauth_base() -> String {
    std::env::var("RIV_OAUTH_BASE").unwrap_or_else(|_| DEFAULT_OAUTH_BASE.into()).trim_end_matches('/').to_string()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn to_credentials(r: TokenResponse, previous_refresh: Option<String>) -> Credentials {
    Credentials {
        access_token: r.access_token,
        // A new refresh token replaces the stored one; otherwise keep the old.
        refresh_token: r.refresh_token.or(previous_refresh),
        id_token: r.id_token,
        expires_at: now() + r.expires_in.unwrap_or(3600),
    }
}

async fn post_token(base: &str, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let resp = reqwest::Client::new()
        .post(format!("{base}/oauth2/token"))
        .timeout(Duration::from_secs(30))
        .form(form)
        .send()
        .await
        .map_err(|e| RivError::auth(format!("token endpoint unreachable: {}", e.without_url())))?;
    if !resp.status().is_success() {
        // Do not echo the body: it may reflect submitted values.
        return Err(RivError::auth(format!("token endpoint returned {}", resp.status())));
    }
    resp.json::<TokenResponse>().await.map_err(|_| RivError::auth("token endpoint returned an unexpected body"))
}

pub async fn refresh(base: &str, refresh_token: &str) -> Result<Credentials> {
    let r = post_token(
        base,
        &[("grant_type", "refresh_token"), ("client_id", CLIENT_ID), ("refresh_token", refresh_token)],
    )
    .await?;
    Ok(to_credentials(r, Some(refresh_token.to_string())))
}

pub async fn revoke(base: &str, refresh_token: &str) -> Result<()> {
    let resp = reqwest::Client::new()
        .post(format!("{base}/oauth2/revoke"))
        .timeout(Duration::from_secs(30))
        .form(&[("token", refresh_token), ("client_id", CLIENT_ID)])
        .send()
        .await
        .map_err(|e| RivError::general(format!("revoke failed: {}", e.without_url())))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(RivError::general(format!("revoke returned {}", resp.status())))
    }
}

pub fn authorize_url(base: &str, redirect_uri: &str, p: &Pkce) -> String {
    let q = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPE)
        .append_pair("identity_provider", "AWSBuilderID")
        .append_pair("code_challenge", &p.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &p.state)
        .finish();
    format!("{base}/oauth2/authorize?{q}")
}

/// Bind the first free loopback port in 8484..=8489.
pub async fn bind_callback_port() -> Result<(tokio::net::TcpListener, u16)> {
    for port in PORTS {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return Ok((l, port));
        }
    }
    Err(RivError::general("ports 8484-8489 are all in use; free one and retry `riv login`"))
}

fn open_browser(url: &str) -> bool {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &[])]
    } else if cfg!(windows) {
        &[("cmd", &["/C", "start", ""])]
    } else {
        &[("xdg-open", &[]), ("wslview", &[])]
    };
    candidates.iter().any(|(cmd, args)| {
        std::process::Command::new(cmd)
            .args(*args)
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
    })
}

type CodeSlot = Arc<Mutex<Option<tokio::sync::oneshot::Sender<std::result::Result<String, String>>>>>;

/// Interactive PKCE sign-in. `announce` receives the URL to show the user.
pub async fn login(base: &str, announce: impl Fn(&str)) -> Result<Credentials> {
    use axum::{Router, extract::Query, response::Html, routing::get};
    use std::collections::HashMap;

    let (listener, port) = bind_callback_port().await?;
    let redirect_uri = format!("http://localhost:{port}/callback");
    let pkce = Pkce::generate();
    let url = authorize_url(base, &redirect_uri, &pkce);

    let (tx, rx) = tokio::sync::oneshot::channel();
    let slot: CodeSlot = Arc::new(Mutex::new(Some(tx)));
    let expected_state = pkce.state.clone();
    let app = Router::new().route(
        "/callback",
        get(move |Query(q): Query<HashMap<String, String>>| {
            let slot = slot.clone();
            let expected = expected_state.clone();
            async move {
                let outcome = if q.get("state") != Some(&expected) {
                    Err("state mismatch".to_string())
                } else if let Some(code) = q.get("code") {
                    Ok(code.clone())
                } else {
                    Err(q.get("error").cloned().unwrap_or_else(|| "no code returned".into()))
                };
                let ok = outcome.is_ok();
                if let Some(tx) = slot.lock().unwrap_or_else(|p| p.into_inner()).take() {
                    let _ = tx.send(outcome);
                }
                Html(if ok {
                    "Signed in. You can close this tab and return to riv."
                } else {
                    "Sign-in failed. Return to riv."
                })
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    announce(&url);
    open_browser(&url);

    let code = tokio::time::timeout(Duration::from_secs(300), rx).await;
    server.abort();
    let code = match code {
        Ok(Ok(Ok(c))) => c,
        Ok(Ok(Err(e))) => return Err(RivError::auth(format!("sign-in failed: {e}"))),
        _ => return Err(RivError::auth("sign-in timed out")),
    };
    let r = post_token(
        base,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", &redirect_uri),
            ("code", &code),
            ("code_verifier", &pkce.verifier),
        ],
    )
    .await?;
    Ok(to_credentials(r, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_has_required_params() {
        let p = Pkce::generate();
        let u = authorize_url(DEFAULT_OAUTH_BASE, "http://localhost:8484/callback", &p);
        for needle in ["code_challenge_method=S256", "identity_provider=AWSBuilderID", "state=", "response_type=code"] {
            assert!(u.contains(needle), "missing {needle}");
        }
        assert!(u.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8484%2Fcallback"));
    }
}
