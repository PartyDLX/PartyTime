//! PKCE and `state` for the authorization-code flow.
//!
//! Split out from the flow itself because this is the part with an exact specification and
//! published test vectors: RFC 7636 defines the transform, and getting it subtly wrong
//! fails at the token endpoint with a bare `invalid_grant` and nothing to debug against.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::rand_core::Rng as _;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// The verifier and the challenge derived from it.
///
/// The verifier never leaves the machine and is zeroized on drop; the challenge is public
/// by the time the authorize request is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    /// The secret half. 43–128 unreserved characters.
    pub verifier: Zeroizing<String>,
    /// `BASE64URL(SHA256(verifier))`, unpadded.
    pub challenge: String,
}

/// The verifier length RFC 7636 allows, and what this module generates.
///
/// 32 random bytes encode to 43 unpadded base64url characters, which is the RFC's
/// minimum; 64 bytes give 86, comfortably inside the 128 ceiling and past the 256-bit
/// entropy floor that makes the verifier unguessable.
const VERIFIER_BYTES: usize = 64;

/// Generates a fresh verifier and its challenge.
#[must_use]
pub fn generate() -> Pkce {
    let mut bytes = [0u8; VERIFIER_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    let verifier = Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes));
    let challenge = challenge_for(&verifier);
    Pkce {
        verifier,
        challenge,
    }
}

/// The S256 challenge for a verifier.
#[must_use]
pub fn challenge_for(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// A random `state`, used to bind the callback to the request that started it.
#[must_use]
pub fn state() -> Zeroizing<String> {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes))
}

/// Whether a verifier is inside the range RFC 7636 allows.
///
/// Checked before the request goes out: an out-of-range verifier is rejected by the
/// authorization server with a generic message, and this is the only place to say why.
#[must_use]
pub fn verifier_is_valid(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
}

/// Whether a returned `state` matches the one this flow sent.
///
/// A mismatch means the callback did not come from the request we made, so it must not be
/// exchanged for tokens.
#[must_use]
pub fn state_matches(expected: &str, received: &str) -> bool {
    // Constant-time is overkill for a value both sides already know, but comparing lengths
    // first keeps the common mismatch case cheap.
    expected.len() == received.len() && expected.bytes().zip(received.bytes()).all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_matches_the_rfc_7636_vector() {
        // RFC 7636 Appendix B.
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_generated_verifier_is_inside_the_allowed_range_and_uses_unreserved_characters() {
        let pkce = generate();
        assert!(
            verifier_is_valid(&pkce.verifier),
            "generated {} chars",
            pkce.verifier.len()
        );
        assert!(pkce.verifier.len() >= 43 && pkce.verifier.len() <= 128);
        assert_eq!(pkce.challenge, challenge_for(&pkce.verifier));
    }

    #[test]
    fn two_generations_differ() {
        assert_ne!(generate().verifier.as_str(), generate().verifier.as_str());
        assert_ne!(state().as_str(), state().as_str());
    }

    #[test]
    fn the_challenge_is_unpadded_base64url() {
        let challenge = generate().challenge;
        assert!(!challenge.contains('='), "PKCE challenges carry no padding");
        assert!(
            challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "challenge must be base64url"
        );
        assert_eq!(
            challenge.len(),
            43,
            "SHA-256 is 32 bytes -> 43 unpadded characters"
        );
    }

    #[test]
    fn out_of_range_or_illegal_verifiers_are_rejected() {
        assert!(!verifier_is_valid(""), "empty");
        assert!(
            !verifier_is_valid(&"a".repeat(42)),
            "one short of the minimum"
        );
        assert!(!verifier_is_valid(&"a".repeat(129)), "one past the maximum");
        assert!(verifier_is_valid(&"a".repeat(43)));
        assert!(verifier_is_valid(&"a".repeat(128)));
        // Reserved characters are not allowed even at the right length.
        // 43 characters, but with a reserved one in the middle.
        assert!(!verifier_is_valid(&"a".repeat(21)));
        assert!(verifier_is_valid(&"a".repeat(43)));
        let with_reserved = format!("{}+/{}", "a".repeat(21), "b".repeat(20));
        assert!(!verifier_is_valid(&with_reserved));
    }

    #[test]
    fn a_state_mismatch_is_refused() {
        let sent = state();
        assert!(state_matches(&sent, &sent));
        assert!(!state_matches(&sent, "something-else-entirely-xx"));
        assert!(!state_matches(&sent, ""));
        // Same length, different content: the case a prefix check would miss.
        let mut tampered = sent.to_string();
        tampered.replace_range(0..1, if tampered.starts_with('a') { "b" } else { "a" });
        assert!(!state_matches(&sent, &tampered));
    }
}
