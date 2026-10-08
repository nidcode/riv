//! Unverified JWT claim reading, used only for display (email) and account hashing (sub).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn claims(jwt: &str) -> Option<serde_json::Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn claim_str(jwt: &str, key: &str) -> Option<String> {
    claims(jwt)?.get(key)?.as_str().map(str::to_string)
}

/// `alice@example.com` -> `al***@example.com` (two characters, so two accounts on one domain can be told apart)
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(2).collect();
            format!("{first}***@{domain}")
        }
        None => "***".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks() {
        assert_eq!(mask_email("alice@example.com"), "al***@example.com");
    }

    #[test]
    fn reads_claims() {
        let p = URL_SAFE_NO_PAD.encode(br#"{"sub":"s1","email":"x@y.z"}"#);
        let jwt = format!("h.{p}.s");
        assert_eq!(claim_str(&jwt, "sub").as_deref(), Some("s1"));
    }
}
