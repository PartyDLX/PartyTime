//! The OAuth 2.0 authorization-code flow with PKCE.
//!
//! PartyTime is a public client: it holds no secret, so PKCE is what binds the code it
//! receives to the request it made. The platform never sees an OpenParty password — the
//! user types that into their browser, on the platform's own page.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::http_client::{AsyncBody, HttpClient};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::discovery::{self, Endpoints};
use crate::pkce::{self, Pkce};
use crate::store::SecretStore;
use crate::token::{TokenError, TokenGrant, TokenSet};

/// The client id registered for this build.
///
/// A build without one cannot sign in, and says so rather than guessing at a client the
/// platform has never heard of.
pub const CLIENT_ID_ENV: &str = "PARTYTIME_OAUTH_CLIENT_ID";

/// Overrides the redirect URI. It must be registered for the client, exactly.
pub const REDIRECT_URI_ENV: &str = "PARTYTIME_OAUTH_REDIRECT";

/// The dev client registered by the platform's own recipe.
pub const DEV_CLIENT_ID: &str = "partytime-dev";

/// The loopback callback the dev client is registered with.
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:1420/oauth/callback";

/// The scopes the console needs. Declaring fewer would change the consent screen.
pub const SCOPES: [&str; 4] = ["profile:read", "channels:read", "parties:read", "publish"];

/// How this build identifies itself to the authorization server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// The registered client id.
    pub client_id: String,
    /// The registered redirect URI, matched exactly by the platform.
    pub redirect_uri: String,
    /// Scopes requested, space-separated when sent.
    pub scopes: Vec<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            client_id: DEV_CLIENT_ID.to_string(),
            redirect_uri: DEFAULT_REDIRECT_URI.to_string(),
            scopes: SCOPES.iter().map(|scope| (*scope).to_string()).collect(),
        }
    }
}

impl ClientConfig {
    /// The configuration for this build, from the environment where it overrides.
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(client_id) = std::env::var(CLIENT_ID_ENV)
            && !client_id.is_empty()
        {
            config.client_id = client_id;
        }
        if let Ok(redirect) = std::env::var(REDIRECT_URI_ENV)
            && !redirect.is_empty()
        {
            config.redirect_uri = redirect;
        }
        config
    }

    /// The scopes as the platform wants them.
    #[must_use]
    pub fn scope_string(&self) -> String {
        self.scopes.join(" ")
    }
}

/// Why this build cannot sign in.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The client id is missing.
    #[error("no OAuth client id is configured (set {CLIENT_ID_ENV})")]
    MissingClientId,
    /// The redirect URI is missing.
    #[error("no OAuth redirect URI is configured (set {REDIRECT_URI_ENV})")]
    MissingRedirectUri,
}

/// Builds the URL the browser is sent to.
///
/// `S256` and `code` only: the platform supports nothing else, and a desktop client that
/// quietly downgraded would be a downgrade nobody asked for.
#[must_use]
pub fn authorize_url(
    endpoints: &Endpoints,
    config: &ClientConfig,
    challenge: &str,
    state: &str,
) -> String {
    let mut url = url::Url::parse(&endpoints.authorization_endpoint)
        .expect("discovery validated this endpoint");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &config.scope_string())
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    url.into()
}

/// What the loopback listener accepts as a result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallbackError {
    /// The user denied consent.
    #[error("access was denied")]
    Denied,
    /// The authorization server reported a failure.
    #[error("the platform refused the request: {code}{}", .description.as_deref().map(|d| format!(" — {d}")).unwrap_or_default())]
    Refused {
        /// The RFC 6749 error code.
        code: String,
        /// `error_description`, when the server sent one.
        description: Option<String>,
    },
    /// The callback did not carry a code.
    #[error("the callback did not carry an authorization code")]
    MissingCode,
    /// The `state` did not match, so the callback is not from the request we made.
    #[error("the sign-in response did not match the request that started it")]
    StateMismatch,
    /// The user closed the browser without finishing.
    #[error("sign-in was cancelled")]
    Cancelled,
}

/// Reads a loopback callback's query string, returning the authorization code.
///
/// A denial is a failure, not an outcome: there is no code to act on, so
/// [`CallbackError::Denied`] is the honest shape for it.
///
/// The `state` check happens here rather than at the exchange, because a mismatched
/// callback must not be turned into a token request at all.
pub fn parse_callback(query: &str, expected_state: &str) -> Result<String, CallbackError> {
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();

    if let Some(code) = params.get("error") {
        return if code == "access_denied" {
            Err(CallbackError::Denied)
        } else {
            Err(CallbackError::Refused {
                code: code.clone(),
                description: params.get("error_description").cloned(),
            })
        };
    }

    match params.get("state") {
        Some(state) if pkce::state_matches(expected_state, state) => {}
        _ => return Err(CallbackError::StateMismatch),
    }

    params
        .get("code")
        .filter(|code| !code.is_empty())
        .cloned()
        .ok_or(CallbackError::MissingCode)
}

/// A grant request, either an exchange or a refresh.
#[derive(Debug, Clone)]
pub enum Grant {
    /// Trading a code plus its verifier for tokens.
    AuthorizationCode {
        /// The code from the callback.
        code: String,
        /// The verifier this flow generated.
        verifier: Zeroizing<String>,
    },
    /// Trading a refresh token for the next pair.
    Refresh {
        /// The refresh token being rotated.
        token: Zeroizing<String>,
    },
}

impl Grant {
    /// The form body the token endpoint expects.
    #[must_use]
    pub fn to_form(&self, config: &ClientConfig) -> String {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.append_pair("client_id", &config.client_id);
        match self {
            Self::AuthorizationCode { code, verifier } => {
                serializer.append_pair("grant_type", "authorization_code");
                serializer.append_pair("code", code);
                serializer.append_pair("code_verifier", verifier.as_str());
                serializer.append_pair("redirect_uri", &config.redirect_uri);
            }
            Self::Refresh { token } => {
                serializer.append_pair("grant_type", "refresh_token");
                serializer.append_pair("refresh_token", token.as_str());
            }
        }
        serializer.finish()
    }
}

/// The token endpoint's refusal, as RFC 6749 defines it.
#[derive(Debug, Clone, Deserialize)]
struct ErrorBody {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// An OAuth failure the console can explain.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OAuthError {
    /// The discovery document was unusable.
    #[error(transparent)]
    Discovery(#[from] discovery::DiscoveryError),
    /// The user declined.
    #[error("you chose not to allow PartyTime to sign in")]
    Denied,
    /// The sign-in was abandoned before it finished.
    #[error("sign-in was cancelled")]
    Cancelled,
    /// The token endpoint refused.
    #[error("the platform refused the sign-in: {0}")]
    Refused(String),
    /// The grant was not usable as a token pair.
    #[error(transparent)]
    Token(#[from] TokenError),
    /// The refresh token could not be kept.
    #[error("could not keep the session: {0}")]
    Store(String),
    /// The network failed.
    #[error("{0}")]
    Transport(String),
}

impl OAuthError {
    /// Whether retrying the same token request could work.
    ///
    /// A refusal is not retryable — a wrong code stays wrong. A transport fault might be,
    /// and a rotation replay is worth one more attempt with a fresh sign-in.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::Refused(message) => message.starts_with("temporarily"),
            _ => false,
        }
    }
}

/// The OAuth half of the client: discovery, token grants, revocation.
pub struct OAuthClient {
    http: Arc<dyn HttpClient>,
    origin: String,
    config: ClientConfig,
    store: Arc<dyn SecretStore>,
    endpoints: std::sync::Mutex<Option<Endpoints>>,
}

impl OAuthClient {
    /// Creates a client against `origin`.
    #[must_use]
    pub fn new(
        http: Arc<dyn HttpClient>,
        origin: impl Into<String>,
        config: ClientConfig,
        store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            http,
            origin: origin.into(),
            config,
            store,
            endpoints: std::sync::Mutex::new(None),
        }
    }

    /// This build's client configuration.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Where the refresh token is being kept.
    #[must_use]
    pub fn store(&self) -> &Arc<dyn SecretStore> {
        &self.store
    }

    /// The platform origin.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Reads the discovery document, once.
    ///
    /// Cached because it cannot change within a launch and every sign-in would otherwise
    /// pay for a round trip the answer cannot have changed.
    pub async fn endpoints(&self) -> Result<Endpoints, OAuthError> {
        if let Some(endpoints) = lock(&self.endpoints).clone() {
            return Ok(endpoints);
        }
        let url = format!(
            "{}{}",
            self.origin.trim_end_matches('/'),
            discovery::DISCOVERY_PATH
        );
        let body = self.get(&url).await?;
        let endpoints = discovery::parse(&body)?;
        discovery::check_endpoint(&endpoints.authorization_endpoint)
            .map_err(OAuthError::Discovery)?;
        discovery::check_endpoint(&endpoints.token_endpoint).map_err(OAuthError::Discovery)?;
        *lock(&self.endpoints) = Some(endpoints.clone());
        Ok(endpoints)
    }

    /// A fresh PKCE verifier and `state` for one sign-in attempt.
    #[must_use]
    pub fn begin(&self) -> Pkce {
        pkce::generate()
    }

    /// Trades an authorization code for tokens and keeps the refresh token.
    pub async fn exchange(&self, code: &str, pkce: &Pkce) -> Result<TokenSet, OAuthError> {
        if !pkce::verifier_is_valid(&pkce.verifier) {
            return Err(OAuthError::Token(TokenError::Empty));
        }
        let tokens = self
            .grant(Grant::AuthorizationCode {
                code: code.to_string(),
                verifier: pkce.verifier.clone(),
            })
            .await?;
        self.store
            .save(tokens.refresh_token())
            .map_err(|error| OAuthError::Store(error.to_string()))?;
        Ok(tokens)
    }

    /// Rotates the refresh token.
    pub async fn refresh(&self, current: &TokenSet) -> Result<TokenSet, OAuthError> {
        let tokens = self
            .grant(Grant::Refresh {
                token: Zeroizing::new(current.refresh_token().to_string()),
            })
            .await?;
        self.store
            .save(tokens.refresh_token())
            .map_err(|error| OAuthError::Store(error.to_string()))?;
        Ok(tokens)
    }

    /// Signs out: revokes the refresh token and forgets it locally.
    ///
    /// A failed revoke still clears the local secret. Leaving a token behind because the
    /// network was down would mean signing out did not sign out.
    pub async fn revoke(&self, current: &TokenSet) -> Result<(), OAuthError> {
        let url = format!("{}/oauth/revoke", self.origin.trim_end_matches('/'));
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("token", current.refresh_token())
            .append_pair("client_id", &self.config.client_id)
            .finish();
        let result = self.post_form(&url, &body).await;
        self.store
            .clear()
            .map_err(|error| OAuthError::Store(error.to_string()))?;
        result.map(|_| ())
    }

    /// Restores a previous session from the credential store.
    ///
    /// The access token is deliberately not stored, so a restored session starts by
    /// refreshing — which is also the only honest answer, since the stored token's age is
    /// unknown.
    pub async fn restore(&self) -> Result<Option<TokenSet>, OAuthError> {
        let Some(refresh) = self
            .store
            .load()
            .map_err(|error| OAuthError::Store(error.to_string()))?
        else {
            return Ok(None);
        };
        if refresh.is_empty() {
            return Ok(None);
        }
        let tokens = self.grant(Grant::Refresh { token: refresh }).await?;
        Ok(Some(tokens))
    }

    async fn grant(&self, grant: Grant) -> Result<TokenSet, OAuthError> {
        let endpoints = self.endpoints().await?;
        let body = grant.to_form(&self.config);
        let raw = self.post_form(&endpoints.token_endpoint, &body).await?;
        let document: TokenGrant = serde_json::from_str(&raw)
            .map_err(|error| OAuthError::Transport(format!("token response: {error}")))?;
        TokenSet::from_grant(&document, std::time::SystemTime::now()).map_err(OAuthError::Token)
    }

    async fn get(&self, url: &str) -> Result<String, OAuthError> {
        let request = gpui_kit::http_client::Request::builder()
            .method("GET")
            .uri(url)
            .header("Accept", "application/json")
            .body(AsyncBody::empty())
            .map_err(|error| OAuthError::Transport(error.to_string()))?;
        let response = self
            .http
            .send(request)
            .await
            .map_err(|error| OAuthError::Transport(describe(error)))?;
        read_body(response).await
    }

    async fn post_form(&self, url: &str, body: &str) -> Result<String, OAuthError> {
        let request = gpui_kit::http_client::Request::builder()
            .method("POST")
            .uri(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(AsyncBody::from(body.to_string()))
            .map_err(|error| OAuthError::Transport(error.to_string()))?;
        let response = self
            .http
            .send(request)
            .await
            .map_err(|error| OAuthError::Transport(describe(error)))?;
        let status = response.status().as_u16();
        let raw = read_body(response).await?;
        if (200..300).contains(&status) {
            return Ok(raw);
        }
        Err(refusal(&raw))
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn read_body(
    mut response: gpui_kit::http_client::Response<AsyncBody>,
) -> Result<String, OAuthError> {
    use futures::AsyncReadExt as _;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .take(1 << 20)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| OAuthError::Transport(error.to_string()))?;
    String::from_utf8(bytes).map_err(|error| OAuthError::Transport(error.to_string()))
}

fn refusal(raw: &str) -> OAuthError {
    match serde_json::from_str::<ErrorBody>(raw) {
        Ok(body) => OAuthError::Refused(match body.error_description {
            Some(description) => format!("{} — {description}", body.error),
            None => body.error,
        }),
        // Not JSON: keep enough to diagnose, never assume the shape.
        Err(_) => OAuthError::Refused(raw.chars().take(200).collect()),
    }
}

fn describe(error: impl std::fmt::Display) -> String {
    let text = error.to_string();
    // A transport error can quote a request; the authorization header must not survive it.
    text.split("Authorization")
        .next()
        .unwrap_or(&text)
        .to_string()
}

/// How long the loopback listener waits for the user to finish in their browser.
pub const DEFAULT_SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Opens `url` in the user's browser.
///
/// Each desktop has its own contract for this, and on Linux that is `xdg-open`, not a
/// search for a browser binary: a flatpak or snap browser is perfectly normal and is not
/// on `PATH` as a binary the `webbrowser` crate recognises. Scanning for binaries makes the
/// sign-in fail on exactly the machines where a browser is installed.
///
/// `webbrowser` remains the fallback for platforms with no such command.
pub fn open_browser(url: &str) -> Result<(), String> {
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "linux") {
        ("xdg-open", &[])
    } else if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("cmd", &["/c", "start"])
    } else {
        return webbrowser::open(url).map_err(describe_open_failure);
    };

    std::process::Command::new(program)
        .args(args)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not run {program}: {error}"))
}

/// Explains why no browser could be opened, and what to do about it.
fn describe_open_failure(error: impl std::fmt::Display) -> String {
    format!("could not open a browser ({error}); open the sign-in link manually")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> Endpoints {
        Endpoints {
            authorization_endpoint: "https://openparty.example/oauth/authorize".into(),
            token_endpoint: "https://openparty.example/oauth/token".into(),
            issuer: Some("https://openparty.example".into()),
        }
    }

    #[test]
    fn the_authorize_url_carries_everything_rfc_7636_requires() {
        let config = ClientConfig::default();
        let url = authorize_url(&endpoints(), &config, "the-challenge", "the-state");
        let parsed = url::Url::parse(&url).expect("valid url");
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();

        assert_eq!(params["response_type"], "code");
        assert_eq!(params["client_id"], DEV_CLIENT_ID);
        assert_eq!(params["redirect_uri"], DEFAULT_REDIRECT_URI);
        assert_eq!(
            params["scope"],
            "profile:read channels:read parties:read publish"
        );
        assert_eq!(params["state"], "the-state");
        assert_eq!(params["code_challenge"], "the-challenge");
        assert_eq!(params["code_challenge_method"], "S256");
        assert!(
            !url.contains("client_secret"),
            "a public client has no secret to send"
        );
    }

    #[test]
    fn a_good_callback_yields_the_code() {
        assert_eq!(
            parse_callback("code=code_abc123&state=s1", "s1").expect("accepted"),
            "code_abc123"
        );
    }

    #[test]
    fn a_callback_from_a_different_request_is_refused() {
        assert_eq!(
            parse_callback("code=code_abc&state=other", "s1"),
            Err(CallbackError::StateMismatch)
        );
        assert_eq!(
            parse_callback("code=code_abc", "s1"),
            Err(CallbackError::StateMismatch)
        );
    }

    #[test]
    fn denying_consent_is_reported_as_a_denial_not_a_failure() {
        assert_eq!(
            parse_callback("error=access_denied&state=s1", "s1"),
            Err(CallbackError::Denied)
        );
        assert!(matches!(
            parse_callback("error=access_denied&state=s1", "s1"),
            Err(CallbackError::Denied)
        ));
    }

    #[test]
    fn other_platform_errors_keep_their_code_and_description() {
        assert_eq!(
            parse_callback("error=invalid_scope&error_description=nope&state=s1", "s1"),
            Err(CallbackError::Refused {
                code: "invalid_scope".into(),
                description: Some("nope".into()),
            })
        );
    }

    #[test]
    fn a_callback_without_a_code_is_not_a_success() {
        assert_eq!(
            parse_callback("state=s1", "s1"),
            Err(CallbackError::MissingCode)
        );
        assert_eq!(
            parse_callback("code=&state=s1", "s1"),
            Err(CallbackError::MissingCode)
        );
    }

    #[test]
    fn the_exchange_body_carries_the_verifier_and_the_registered_redirect() {
        let config = ClientConfig::default();
        let body = Grant::AuthorizationCode {
            code: "code_abc".into(),
            verifier: Zeroizing::new("v".repeat(43)),
        }
        .to_form(&config);
        let params: std::collections::HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();

        assert_eq!(params["grant_type"], "authorization_code");
        assert_eq!(params["code"], "code_abc");
        assert_eq!(params["redirect_uri"], DEFAULT_REDIRECT_URI);
        assert_eq!(params["client_id"], DEV_CLIENT_ID);
        assert_eq!(params["code_verifier"].len(), 43);
        // The redirect URI contains `:` and `/`; both must survive encoding.
        assert_eq!(
            params["redirect_uri"],
            "http://127.0.0.1:1420/oauth/callback"
        );
    }

    #[test]
    fn the_refresh_body_carries_only_what_a_refresh_needs() {
        let config = ClientConfig::default();
        let body = Grant::Refresh {
            token: Zeroizing::new("pr_abc".into()),
        }
        .to_form(&config);
        let params: std::collections::HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();

        assert_eq!(params["grant_type"], "refresh_token");
        assert_eq!(params["refresh_token"], "pr_abc");
        assert!(!params.contains_key("code"));
        assert!(!params.contains_key("redirect_uri"));
    }

    #[test]
    fn a_refusal_is_not_retryable_but_a_transport_fault_is() {
        assert!(!OAuthError::Refused("invalid_grant".into()).is_retryable());
        assert!(OAuthError::Transport("connection reset".into()).is_retryable());
        assert!(!OAuthError::Denied.is_retryable());
        assert!(!OAuthError::Cancelled.is_retryable());
    }

    #[test]
    fn a_transport_error_never_carries_an_authorization_header() {
        let described = describe("request failed with header Authorization: Bearer pt_secret");
        assert!(!described.contains("pt_secret"), "{described}");
        assert!(!described.contains("Authorization"), "{described}");
    }

    #[test]
    fn a_non_json_refusal_is_kept_rather_than_reported_as_something_else() {
        assert_eq!(
            refusal("upstream is down"),
            OAuthError::Refused("upstream is down".into())
        );
        assert_eq!(
            refusal(r#"{"error":"invalid_grant","error_description":"code already used"}"#),
            OAuthError::Refused("invalid_grant — code already used".into())
        );
    }

    #[test]
    fn the_default_configuration_matches_the_registered_dev_client() {
        let config = ClientConfig::from_env();
        assert_eq!(config.client_id, DEV_CLIENT_ID);
        assert_eq!(config.redirect_uri, "http://127.0.0.1:1420/oauth/callback");
        assert_eq!(
            config.scopes,
            vec!["profile:read", "channels:read", "parties:read", "publish"]
        );
    }
}
