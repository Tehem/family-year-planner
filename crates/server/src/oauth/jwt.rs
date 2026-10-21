//! Access tokens: JWTs signed with EdDSA (Ed25519) from `OAUTH_SIGNING_KEY` (§11.4).

use base64::Engine;
use ed25519_dalek::SigningKey;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::crypto::{b64url, sha256};

/// PKCS#8 prefix of an Ed25519 private key; the 32-byte seed follows.
const PKCS8_ED25519_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

pub struct TokenKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
    kid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccessClaims {
    pub iss: String,
    /// The protected resource (`{base_url}/mcp`).
    pub aud: String,
    /// The member's email.
    pub sub: String,
    pub client_id: String,
    /// Display name of the client (extra claim, avoids a lookup on every request).
    #[serde(default)]
    pub client_name: String,
    /// Space-separated scopes.
    pub scope: String,
    pub grant_id: String,
    pub jti: String,
    pub iat: i64,
    pub exp: i64,
}

impl AccessClaims {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }
}

impl TokenKeys {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut der = PKCS8_ED25519_PREFIX.to_vec();
        der.extend_from_slice(seed);
        let public = SigningKey::from_bytes(seed).verifying_key().to_bytes();
        TokenKeys {
            encoding: EncodingKey::from_ed_der(&der),
            decoding: DecodingKey::from_ed_der(&public),
            kid: b64url(&sha256(&public)[..8]),
        }
    }

    /// `OAUTH_SIGNING_KEY`: base64 of 32 random bytes.
    pub fn from_base64(b64: &str) -> Result<Self, String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| "OAUTH_SIGNING_KEY must be base64".to_string())?;
        let seed: [u8; 32] = bytes.try_into().map_err(|_| {
            "OAUTH_SIGNING_KEY must decode to exactly 32 bytes (openssl rand -base64 32)"
                .to_string()
        })?;
        Ok(TokenKeys::from_seed(&seed))
    }

    pub fn random() -> Self {
        let mut seed = [0u8; 32];
        rand::fill(&mut seed[..]);
        TokenKeys::from_seed(&seed)
    }

    pub fn sign(&self, claims: &AccessClaims) -> Result<String, String> {
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("at+jwt".to_string());
        header.kid = Some(self.kid.clone());
        jsonwebtoken::encode(&header, claims, &self.encoding).map_err(|e| e.to_string())
    }

    /// Verify signature, expiry, issuer and audience.
    pub fn verify(&self, token: &str, issuer: &str, audience: &str) -> Option<AccessClaims> {
        let mut v = Validation::new(Algorithm::EdDSA);
        v.set_issuer(&[issuer]);
        v.set_audience(&[audience]);
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        v.leeway = 5;
        jsonwebtoken::decode::<AccessClaims>(token, &self.decoding, &v)
            .ok()
            .map(|d| d.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(exp_in: i64) -> AccessClaims {
        let now = chrono::Utc::now().timestamp();
        AccessClaims {
            iss: "https://famille.example.org".into(),
            aud: "https://famille.example.org/mcp".into(),
            sub: "alex@example.com".into(),
            client_id: "c1".into(),
            client_name: "Claude".into(),
            scope: "calendar:read calendar:write".into(),
            grant_id: "g1".into(),
            jti: "j1".into(),
            iat: now,
            exp: now + exp_in,
        }
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let keys = TokenKeys::random();
        let c = claims(3600);
        let t = keys.sign(&c).unwrap();
        assert_eq!(keys.verify(&t, &c.iss, &c.aud), Some(c.clone()));
        assert!(c.has_scope("calendar:write") && !c.has_scope("calendar"));
    }

    #[test]
    fn header_declares_eddsa() {
        let keys = TokenKeys::random();
        let t = keys.sign(&claims(60)).unwrap();
        let h = jsonwebtoken::decode_header(&t).unwrap();
        assert_eq!(h.alg, Algorithm::EdDSA);
        assert_eq!(h.typ.as_deref(), Some("at+jwt"));
        assert!(h.kid.is_some());
    }

    #[test]
    fn wrong_key_issuer_audience_or_expiry_fail() {
        let keys = TokenKeys::random();
        let c = claims(3600);
        let t = keys.sign(&c).unwrap();
        assert!(TokenKeys::random().verify(&t, &c.iss, &c.aud).is_none());
        assert!(keys.verify(&t, "https://other", &c.aud).is_none());
        assert!(
            keys.verify(&t, &c.iss, "https://famille.example.org/other")
                .is_none()
        );
        let expired = keys.sign(&claims(-3600)).unwrap();
        assert!(keys.verify(&expired, &c.iss, &c.aud).is_none());
        assert!(keys.verify("garbage", &c.iss, &c.aud).is_none());
        // Tampered payload.
        let parts: Vec<&str> = t.split('.').collect();
        let forged = format!("{}.{}.{}", parts[0], b64url(br#"{"sub":"evil"}"#), parts[2]);
        assert!(keys.verify(&forged, &c.iss, &c.aud).is_none());
    }

    #[test]
    fn signing_key_comes_from_the_environment_seed() {
        let b64 = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        let a = TokenKeys::from_base64(&b64).unwrap();
        let b = TokenKeys::from_base64(&b64).unwrap();
        let c = claims(60);
        let t = a.sign(&c).unwrap();
        assert!(
            b.verify(&t, &c.iss, &c.aud).is_some(),
            "same seed → same key"
        );
        assert!(TokenKeys::from_base64("nope!").is_err());
        assert!(
            TokenKeys::from_base64(&base64::engine::general_purpose::STANDARD.encode([1u8; 16]))
                .is_err()
        );
    }
}
