//! The PartyTime API client.
//!
//! `GET/POST /api/partytime/v1/*` with a bearer access token. Two behaviours worth
//! naming:
//!
//! * **Ids go into the path exactly as received.** The list endpoint returns a bare id and
//!   the publish endpoint echoes it back prefixed; both are used verbatim, because
//!   normalising either produces a 404.
//! * **A 401 refreshes once and retries once.** After that the session is over and the
//!   console asks the user to sign in again, rather than looping.

use std::sync::{Arc, Mutex, MutexGuard};

use futures::AsyncReadExt as _;
use gpui_kit::http_client::{AsyncBody, HttpClient, Request, Response};
use serde::de::DeserializeOwned;

use crate::error::PartyError;
use crate::kind::PublishKind;
use crate::models::{ChannelList, DeclareInput, Me, PartyDetail, PartyList, PublishGrant};
use crate::oauth::{OAuthClient, OAuthError};
use crate::token::{TokenError, TokenSet};

/// The API's base path, fixed for the life of v1.
pub const BASE_PATH: &str = "/api/partytime/v1";

/// The largest response the console will read.
const MAX_RESPONSE_BYTES: u64 = 1 << 20;

/// A client for the PartyTime API, holding the current access token.
pub struct ApiClient {
    http: Arc<dyn HttpClient>,
    origin: String,
    oauth: Arc<OAuthClient>,
    tokens: Mutex<Option<TokenSet>>,
}

impl ApiClient {
    /// Creates a client, sharing the OAuth client so a 401 can be answered by a refresh.
    #[must_use]
    pub fn new(
        http: Arc<dyn HttpClient>,
        origin: impl Into<String>,
        oauth: Arc<OAuthClient>,
    ) -> Self {
        Self {
            http,
            origin: origin.into(),
            oauth,
            tokens: Mutex::new(None),
        }
    }

    /// The path for one of a party's endpoints.
    ///
    /// `id` is placed in the path exactly as the platform sent it. Normalising the
    /// `party:` prefix — in either direction — 404s, so this function does not touch it.
    #[must_use]
    pub fn party_path(id: &str, suffix: &str) -> String {
        let suffix = suffix.trim_start_matches('/');
        if suffix.is_empty() {
            format!("{BASE_PATH}/parties/{id}")
        } else {
            format!("{BASE_PATH}/parties/{id}/{suffix}")
        }
    }

    /// Adopts a token pair, replacing whatever was held.
    pub fn set_tokens(&self, tokens: TokenSet) {
        *self.lock() = Some(tokens);
    }

    /// Whether an access token is held and still usable.
    #[must_use]
    pub fn has_usable_token(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|tokens| !tokens.is_expired(std::time::SystemTime::now()))
    }

    /// The current tokens, if any.
    #[must_use]
    pub fn tokens(&self) -> Option<TokenSet> {
        self.lock().clone()
    }

    /// Forgets the tokens. The stored refresh token is left alone — signing out of the API
    /// is not signing out of the account.
    pub fn clear_tokens(&self) {
        *self.lock() = None;
    }

    /// `GET /me` — who am I, and with what scopes.
    pub async fn me(&self) -> Result<Me, PartyError> {
        self.get(&format!("{BASE_PATH}/me")).await
    }

    /// `GET /parties` — what this user can stream to.
    pub async fn parties(&self, live_only: bool) -> Result<PartyList, PartyError> {
        let path = if live_only {
            format!("{BASE_PATH}/parties?live=1")
        } else {
            format!("{BASE_PATH}/parties")
        };
        self.get(&path).await
    }

    /// `GET /parties/{id}` — one party in detail.
    pub async fn party(&self, id: &str) -> Result<PartyDetail, PartyError> {
        self.get(&Self::party_path(id, "")).await
    }

    /// `GET /channels` — the channels this user owns or edits.
    pub async fn channels(&self) -> Result<ChannelList, PartyError> {
        self.get(&format!("{BASE_PATH}/channels")).await
    }

    /// `POST /parties/{id}/inputs` — declare a publish input for approval.
    ///
    /// Declaring publishes nothing. It creates a pending row that only owner or moderator
    /// approval turns into something sendable.
    pub async fn declare_input(
        &self,
        id: &str,
        kind: PublishKind,
        label: Option<&str>,
    ) -> Result<(), PartyError> {
        let body = serde_json::to_string(&DeclareInput {
            kind,
            label: label.filter(|value| !value.is_empty()).map(str::to_string),
        })
        .map_err(|error| PartyError::Transport(format!("could not encode the request: {error}")))?;
        self.post(&Self::party_path(id, "inputs"), Some(body)).await
    }

    /// `POST /parties/{id}/live` — take the party live. Owner or active director.
    pub async fn go_live(&self, id: &str) -> Result<(), PartyError> {
        self.post(&Self::party_path(id, "live"), None).await
    }

    /// `POST /parties/{id}/publish` — mint the credentials for one publish attempt.
    ///
    /// The grant's token is valid for sixty seconds and is single use. Mint it immediately
    /// before offering to the SDP, never earlier.
    pub async fn publish(&self, id: &str) -> Result<PublishGrant, PartyError> {
        self.post(&Self::party_path(id, "publish"), None).await
    }

    /// `POST /parties/{id}/end` — end the live session.
    pub async fn end_session(&self, id: &str, session_id: Option<&str>) -> Result<(), PartyError> {
        let mut path = Self::party_path(id, "end");
        if let Some(session_id) = session_id {
            let separator = if path.contains('?') { '&' } else { '?' };
            path.push(separator);
            path.push_str("sessionId=");
            path.push_str(
                &url::form_urlencoded::byte_serialize(session_id.as_bytes()).collect::<String>(),
            );
        }
        self.post(&path, None).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, PartyError> {
        self.run("GET", path, None).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: Option<String>,
    ) -> Result<T, PartyError> {
        self.run("POST", path, body).await
    }

    /// One call, refreshing and retrying once if the token has gone stale.
    async fn run<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> Result<T, PartyError> {
        match self.attempt(method, path, body.clone()).await {
            Err(PartyError::Unauthenticated) => {
                self.refresh().await?;
                self.attempt(method, path, body).await
            }
            other => other,
        }
    }

    /// Replaces the access token from the stored refresh token.
    pub async fn refresh(&self) -> Result<TokenSet, PartyError> {
        let current = self.lock().clone();
        let refreshed = match &current {
            Some(tokens) => self.oauth.refresh(tokens).await,
            // Nothing in hand: try to pick a previous session back up from the keychain.
            None => match self.oauth.restore().await {
                Ok(Some(tokens)) => Ok(tokens),
                Ok(None) => Err(OAuthError::Cancelled),
                Err(error) => Err(error),
            },
        };
        let tokens = refreshed.map_err(PartyError::from)?;
        *self.lock() = Some(tokens.clone());
        Ok(tokens)
    }

    async fn attempt<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> Result<T, PartyError> {
        let url = format!("{}{}", self.origin.trim_end_matches('/'), path);
        let Some(access) = self
            .lock()
            .as_ref()
            .map(TokenSet::access_token)
            .map(str::to_string)
        else {
            return Err(PartyError::Unauthenticated);
        };

        let mut builder = Request::builder()
            .method(method)
            .uri(&url)
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {access}"));
        if body.is_some() {
            builder = builder.header("Content-Type", "application/json");
        }
        let request = builder
            .body(match body {
                Some(body) => AsyncBody::from(body),
                None => AsyncBody::empty(),
            })
            .map_err(|error| PartyError::Transport(error.to_string()))?;

        let mut response: Response<AsyncBody> = self
            .http
            .send(request)
            .await
            .map_err(|error| PartyError::Transport(redact(&error.to_string())))?;
        let status = response.status().as_u16();
        let bytes = read_body(&mut response).await?;
        let text = String::from_utf8_lossy(&bytes).into_owned();

        match status {
            401 => Err(PartyError::Unauthenticated),
            403 => Err(if text.contains("insufficient_scope") {
                PartyError::InsufficientScope
            } else {
                PartyError::from_response(status, &text)
            }),
            s if (200..300).contains(&s) => serde_json::from_str(&text).map_err(|error| {
                // Endpoints that answer with an empty body decode to unit.
                if text.trim().is_empty() {
                    // Nothing to hand back; the caller asked for `()`.
                    return PartyError::UnexpectedResponse {
                        detail: "empty body".into(),
                    };
                }
                PartyError::UnexpectedResponse {
                    detail: error.to_string(),
                }
            }),
            s => Err(PartyError::from_response(s, &text)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<TokenSet>> {
        self.tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

async fn read_body(response: &mut Response<AsyncBody>) -> Result<Vec<u8>, PartyError> {
    let mut bytes = Vec::new();
    response
        .body_mut()
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| PartyError::Transport(redact(&error.to_string())))?;
    Ok(bytes)
}

/// Removes anything shaped like a bearer token from a message before it can be logged.
fn redact(message: &str) -> String {
    message
        .split("Authorization")
        .next()
        .unwrap_or(message)
        .split("Bearer ")
        .next()
        .unwrap_or(message)
        .to_string()
}

impl From<OAuthError> for PartyError {
    fn from(error: OAuthError) -> Self {
        match error {
            OAuthError::Denied => Self::SignInDenied,
            OAuthError::Cancelled => Self::SignInCancelled,
            OAuthError::Refused(message) => Self::SignInRefused(message),
            OAuthError::Token(TokenError::NoRefreshToken)
            | OAuthError::Token(TokenError::Empty) => {
                Self::SignInRefused("the platform did not return a usable session".into())
            }
            OAuthError::Token(TokenError::UnexpectedTokenType(kind)) => {
                Self::SignInRefused(format!("the platform returned a {kind} token"))
            }
            OAuthError::Store(message) => {
                Self::SignInRefused(format!("could not keep the session: {message}"))
            }
            OAuthError::Transport(message) => Self::Transport(message),
            OAuthError::Discovery(error) => Self::Transport(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_party_id_reaches_the_path_exactly_as_received() {
        // Bare, from the list endpoint.
        assert_eq!(
            ApiClient::party_path("wlyayz1ytl2u822bifb4", "publish"),
            "/api/partytime/v1/parties/wlyayz1ytl2u822bifb4/publish"
        );
        // Prefixed, as the publish grant echoes it back. Neither form is normalised.
        assert_eq!(
            ApiClient::party_path("party:wlyayz1ytl2u822bifb4", "live"),
            "/api/partytime/v1/parties/party:wlyayz1ytl2u822bifb4/live"
        );
        assert_eq!(
            ApiClient::party_path("p1", ""),
            "/api/partytime/v1/parties/p1"
        );
        assert_eq!(
            ApiClient::party_path("p1", "/inputs"),
            "/api/partytime/v1/parties/p1/inputs"
        );
    }

    #[test]
    fn the_base_path_is_the_v1_surface() {
        assert_eq!(BASE_PATH, "/api/partytime/v1");
    }

    #[test]
    fn a_message_carrying_a_bearer_token_is_scrubbed_before_it_can_be_logged() {
        let message = redact("connect failed: header Authorization: Bearer pt_a1b2c3");
        assert!(!message.contains("pt_a1b2c3"), "{message}");
        assert!(!message.contains("Bearer"), "{message}");
    }

    #[test]
    fn a_message_without_a_token_survives_untouched() {
        let message = "connection refused";
        assert_eq!(redact(message), message);
    }
}
