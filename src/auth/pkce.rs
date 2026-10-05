//! PKCE (RFC 7636, S256), hand-written on sha2 + base64 + rand.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt;
use sha2::{Digest, Sha256};

pub fn b64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Fresh CSPRNG bytes, base64url encoded (unreserved chars only).
pub fn random_token(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    rand::rng().fill(&mut buf[..]);
    b64url(&buf)
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

pub fn challenge_for(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

impl Pkce {
    /// New verifier (86 chars) + challenge + state, fresh for every attempt.
    pub fn generate() -> Self {
        let verifier = random_token(64);
        let challenge = challenge_for(&verifier);
        Self { verifier, challenge, state: random_token(24) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_vector() {
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_shape_and_freshness() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert!((43..=128).contains(&a.verifier.len()));
        assert!(a.verifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.state, b.state);
    }
}
