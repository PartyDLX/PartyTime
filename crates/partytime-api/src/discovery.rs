//! The authorization server's discovery document.
//!
//! `GET /.well-known/oauth-authorization-server` says where the endpoints are. Reading it
//! rather than hard-coding them means a deployment that moves, or a staging origin, needs
//! no change here.

use serde::Deserialize;

/// The endpoints a sign-in needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Where the browser is sent to sign in and consent.
    pub authorization_endpoint: String,
    /// Where codes and refresh tokens are exchanged for access tokens.
    pub token_endpoint: String,
    /// The issuer this document describes.
    pub issuer: Option<String>,
}

/// Reads the discovery document.
pub fn parse(body: &str) -> Result<Endpoints, DiscoveryError> {
    let document: Document =
        serde_json::from_str(body).map_err(|error| DiscoveryError::Malformed(error.to_string()))?;
    Ok(Endpoints {
        authorization_endpoint: document.authorization_endpoint,
        token_endpoint: document.token_endpoint,
        issuer: document.issuer,
    })
}

/// The discovery document as the platform serves it (RFC 8414).
#[derive(Debug, Deserialize)]
struct Document {
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    issuer: Option<String>,
}

/// The well-known path the document lives at.
pub const DISCOVERY_PATH: &str = "/.well-known/oauth-authorization-server";

/// Why the discovery document could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    /// The document was not the shape RFC 8414 describes.
    #[error("the authorization server metadata is not usable: {0}")]
    Malformed(String),
    /// An endpoint in the document was not a usable absolute http(s) URL.
    #[error("the authorization server advertised an endpoint the console cannot use: {0}")]
    UnusableEndpoint(String),
    /// The document could not be fetched.
    #[error("could not reach the authorization server: {0}")]
    Unreachable(String),
}

/// Rejects an endpoint the console must not be talked into calling.
///
/// An absolute `http`/`https` URL with no embedded credentials. A `file:` or `data:` URL
/// here, or `https://user:pass@host`, would mean a compromised or misconfigured discovery
/// document redirecting the sign-in — and its tokens — somewhere they should not go.
pub fn check_endpoint(url: &str) -> Result<(), DiscoveryError> {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return Err(DiscoveryError::UnusableEndpoint(url.to_string()));
    };
    if rest.contains('@') {
        return Err(DiscoveryError::UnusableEndpoint(url.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = r#"{
      "issuer": "https://openparty.example",
      "authorization_endpoint": "https://openparty.example/oauth/authorize",
      "token_endpoint": "https://openparty.example/oauth/token",
      "revocation_endpoint": "https://openparty.example/oauth/revoke",
      "response_types_supported": ["code"],
      "grant_types_supported": ["authorization_code", "refresh_token"],
      "code_challenge_methods_supported": ["S256"]
    }"#;

    #[test]
    fn the_document_yields_the_two_endpoints_a_sign_in_needs() {
        let endpoints = parse(DOCUMENT).expect("parses");
        assert_eq!(
            endpoints.authorization_endpoint,
            "https://openparty.example/oauth/authorize"
        );
        assert_eq!(
            endpoints.token_endpoint,
            "https://openparty.example/oauth/token"
        );
        assert_eq!(
            endpoints.issuer.as_deref(),
            Some("https://openparty.example")
        );
    }

    #[test]
    fn a_document_missing_the_endpoints_is_refused() {
        assert!(matches!(parse("{}"), Err(DiscoveryError::Malformed(_))));
        assert!(matches!(
            parse("not json"),
            Err(DiscoveryError::Malformed(_))
        ));
        // An issuer on its own is not enough to sign in with.
        let issuer_only = r#"{"issuer":"https://openparty.example"}"#;
        assert!(matches!(
            parse(issuer_only),
            Err(DiscoveryError::Malformed(_))
        ));
    }

    #[test]
    fn unknown_fields_are_ignored_because_the_document_is_additive() {
        let extended = r#"{
          "authorization_endpoint": "https://o/a",
          "token_endpoint": "https://o/t",
          "something_new": {"added": "later"}
        }"#;
        let endpoints = parse(extended).expect("parses");
        assert_eq!(endpoints.token_endpoint, "https://o/t");
        assert_eq!(endpoints.issuer, None);
    }

    #[test]
    fn https_and_plain_http_endpoints_are_both_accepted_for_local_development() {
        assert!(check_endpoint("https://openparty.example/oauth/authorize").is_ok());
        assert!(check_endpoint("http://127.0.0.1:5174/oauth/authorize").is_ok());
    }

    #[test]
    fn an_endpoint_that_is_not_http_is_refused() {
        for url in [
            "file:///etc/passwd",
            "data:text/html,<script>",
            "javascript:alert(1)",
            "openparty.example/oauth",
            "",
        ] {
            assert!(check_endpoint(url).is_err(), "{url} should be refused");
        }
    }

    #[test]
    fn an_endpoint_carrying_credentials_is_refused() {
        assert!(check_endpoint("https://user:pw@openparty.example/oauth").is_err());
    }
}
