//! The tokens the platform issues, and when they need replacing.
//!
//! Two lifetimes with very different characters: the access token is an hour and lives in
//! memory, the refresh token is thirty days and lives in the OS keychain. The 60-second
//! media token from `POST /parties/{id}/publish` is not one of these — it is minted per
//! publish attempt and deliberately has no representation here.

use std::time::{Duration, SystemTime};

use serde::Deserialize;
use zeroize::Zeroizing;

/// How long before expiry a refresh is considered due.
///
/// The platform issues hour-long access tokens; refreshing a minute early leaves room for
/// one retry without straddling the boundary.
pub const REFRESH_LEEWAY: Duration = Duration::from_secs(60);

/// The response to a successful token grant.
#[derive(Clone, Deserialize)]
pub struct TokenGrant {
    /// The bearer token for the PartyTime API.
    pub access_token: String,
    /// The rotating token used to mint the next pair.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Seconds until the access token expires.
    pub expires_in: u64,
    /// The scopes actually granted, which may be fewer than were asked for.
    #[serde(default)]
    pub scope: String,
    /// Always `Bearer`; recorded so an unexpected type is visible rather than assumed.
    #[serde(rename = "token_type", default)]
    pub token_type: String,
}

/// A live pair of tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSet {
    access: Zeroizing<String>,
    refresh: Zeroizing<String>,
    expires_at: SystemTime,
    scope: String,
}

impl TokenSet {
    /// Builds a token set from a grant, anchoring expiry to `now`.
    ///
    /// A grant without a refresh token is refused: without one, signing in again would be
    /// the only way back after the access token expires, and the flow promises a
    /// thirty-day session.
    pub fn from_grant(grant: &TokenGrant, now: SystemTime) -> Result<Self, TokenError> {
        if !grant.token_type.is_empty() && !grant.token_type.eq_ignore_ascii_case("bearer") {
            return Err(TokenError::UnexpectedTokenType(grant.token_type.clone()));
        }
        let refresh = grant
            .refresh_token
            .clone()
            .ok_or(TokenError::NoRefreshToken)
            .map(Zeroizing::new)?;
        if refresh.is_empty() || grant.access_token.is_empty() {
            return Err(TokenError::Empty);
        }
        Ok(Self {
            access: Zeroizing::new(grant.access_token.clone()),
            refresh,
            expires_at: now + Duration::from_secs(grant.expires_in),
            scope: grant.scope.clone(),
        })
    }

    /// Builds a set from parts, for a session restored from the keychain.
    ///
    /// `expires_in` is the *remaining* lifetime, which is what matters once a refresh token
    /// has been sitting in storage for a while.
    #[must_use]
    pub fn from_parts(
        access: String,
        refresh: String,
        expires_in: Duration,
        scope: String,
        now: SystemTime,
    ) -> Self {
        Self {
            access: Zeroizing::new(access),
            refresh: Zeroizing::new(refresh),
            expires_at: now + expires_in,
            scope,
        }
    }

    /// The bearer token, for the `Authorization` header.
    #[must_use]
    pub fn access_token(&self) -> &str {
        &self.access
    }

    /// The refresh token. Only ever handed to the store or the token endpoint.
    #[must_use]
    pub fn refresh_token(&self) -> &str {
        &self.refresh
    }

    /// The scopes the platform actually granted.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// When the access token stops working.
    #[must_use]
    pub fn expires_at(&self) -> SystemTime {
        self.expires_at
    }

    /// Whether the access token has expired.
    #[must_use]
    pub fn is_expired(&self, now: SystemTime) -> bool {
        now >= self.expires_at
    }

    /// Whether a refresh is due — expired, or close enough that a retry could straddle it.
    #[must_use]
    pub fn needs_refresh(&self, now: SystemTime) -> bool {
        self.expires_at
            .duration_since(now)
            .map_or(true, |remaining| remaining <= REFRESH_LEEWAY)
    }
}

/// Why a token grant could not be turned into a usable pair.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// The grant carried no refresh token.
    #[error("the platform returned no refresh token")]
    NoRefreshToken,
    /// A token was empty.
    #[error("the platform returned an empty token")]
    Empty,
    /// The grant was not a bearer token.
    #[error("the platform returned token type {0:?}, expected bearer")]
    UnexpectedTokenType(String),
}

impl std::fmt::Debug for TokenGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The tokens themselves must never reach a log line or a panic message.
        f.debug_struct("TokenGrant")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .field("token_type", &self.token_type)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)
    }

    fn grant() -> TokenGrant {
        TokenGrant {
            access_token: "pt_access".into(),
            refresh_token: Some("pr_refresh".into()),
            expires_in: 3600,
            scope: "profile:read parties:read publish".into(),
            token_type: "Bearer".into(),
        }
    }

    #[test]
    fn a_grant_becomes_a_pair_with_the_stated_lifetime() {
        let tokens = TokenSet::from_grant(&grant(), now()).expect("grant is usable");
        assert_eq!(tokens.access_token(), "pt_access");
        assert_eq!(tokens.refresh_token(), "pr_refresh");
        assert_eq!(tokens.expires_at(), now() + Duration::from_secs(3600));
        assert!(!tokens.is_expired(now()));
    }

    #[test]
    fn a_refresh_is_due_one_minute_before_expiry_not_at_it() {
        let tokens = TokenSet::from_grant(&grant(), now()).expect("grant is usable");
        assert!(!tokens.needs_refresh(now()));
        assert!(!tokens.needs_refresh(now() + Duration::from_secs(3599 - 60)));
        assert!(tokens.needs_refresh(now() + Duration::from_secs(3600 - 60)));
        assert!(tokens.needs_refresh(now() + Duration::from_secs(3600)));
        assert!(tokens.is_expired(now() + Duration::from_secs(3600)));
    }

    #[test]
    fn a_clock_that_has_gone_backwards_does_not_look_infinite() {
        let tokens = TokenSet::from_grant(&grant(), now()).expect("grant is usable");
        let before = now() - Duration::from_secs(600);
        // The expiry is still in the future, so the token is fine; this guards the
        // `duration_since` branch that would otherwise be assumed unreachable.
        assert!(!tokens.needs_refresh(before));
    }

    #[test]
    fn a_grant_without_a_refresh_token_is_refused() {
        let mut without = grant();
        without.refresh_token = None;
        assert_eq!(
            TokenSet::from_grant(&without, now()),
            Err(TokenError::NoRefreshToken)
        );
    }

    #[test]
    fn an_empty_token_is_refused() {
        let mut empty_access = grant();
        empty_access.access_token = String::new();
        assert_eq!(
            TokenSet::from_grant(&empty_access, now()),
            Err(TokenError::Empty)
        );

        let mut empty_refresh = grant();
        empty_refresh.refresh_token = Some(String::new());
        assert_eq!(
            TokenSet::from_grant(&empty_refresh, now()),
            Err(TokenError::Empty)
        );
    }

    #[test]
    fn an_unexpected_token_type_is_named_rather_than_guessed_at() {
        let mut other = grant();
        other.token_type = "mac".into();
        assert_eq!(
            TokenSet::from_grant(&other, now()),
            Err(TokenError::UnexpectedTokenType("mac".into()))
        );
    }

    #[test]
    fn a_missing_token_type_is_treated_as_bearer() {
        let mut absent = grant();
        absent.token_type = String::new();
        assert!(TokenSet::from_grant(&absent, now()).is_ok());
    }

    #[test]
    fn debugging_a_grant_never_reveals_either_token() {
        let rendered = format!("{:?}", grant());
        assert!(!rendered.contains("pt_access"), "{rendered}");
        assert!(!rendered.contains("pr_refresh"), "{rendered}");
        assert!(rendered.contains("<redacted>"));
        assert!(
            rendered.contains("3600"),
            "the lifetime is still useful to see"
        );
    }

    #[test]
    fn a_restored_pair_uses_its_remaining_lifetime() {
        let tokens = TokenSet::from_parts(
            "a".into(),
            "r".into(),
            Duration::from_secs(120),
            "publish".into(),
            now(),
        );
        assert!(!tokens.needs_refresh(now()));
        assert!(tokens.needs_refresh(now() + Duration::from_secs(90)));
    }
}
