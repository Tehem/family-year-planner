//! Small helpers for random tokens and hashing.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// `n` cryptographically secure random bytes.
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::fill(&mut buf[..]);
    buf
}

/// base64url (no padding) of `n` random bytes.
pub fn random_token(n: usize) -> String {
    URL_SAFE_NO_PAD.encode(random_bytes(n))
}

pub fn sha256(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}

pub fn b64url(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

pub fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

/// PKCE S256 challenge of a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    b64url(&sha256(verifier.as_bytes()))
}

/// Constant-time comparison of two byte strings.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_url_safe() {
        let a = random_token(32);
        let b = random_token(32);
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(
            a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn constant_time_equality() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
