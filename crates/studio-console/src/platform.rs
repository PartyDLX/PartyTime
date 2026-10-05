//! What onboarding needs from the platform, and the real implementation.
//!
//! The trait exists so the welcome and party screens can be driven in a test without a
//! browser or a server. The production implementation wires the OAuth client, the API
//! client, the loopback listener and the system browser together.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use studio_party::{
    ApiClient, CallbackListener, ClientConfig, Me, OAuthClient, PartyDetail, PartyError, PartyList,
    TokenSet, default_store,
};

/// A future the onboarding view can await.
pub type PlatformFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// What the onboarding screens need from OpenParty.
pub trait Platform: Send + Sync + 'static {
    /// Runs the whole browser sign-in: PKCE, the consent screen, the token exchange, and
    /// the refresh token into the credential store.
    fn start_sign_in(&self) -> PlatformFuture<Result<TokenSet, PartyError>>;

    /// Who is signed in, with the scopes the token actually carries.
    fn me(&self) -> PlatformFuture<Result<Me, PartyError>>;

    /// The parties this user can stream to.
    fn parties(&self, live_only: bool) -> PlatformFuture<Result<PartyList, PartyError>>;

    /// One party in detail.
    fn party(&self, id: String) -> PlatformFuture<Result<PartyDetail, PartyError>>;

    /// Revokes the refresh token and forgets it.
    fn sign_out(&self) -> PlatformFuture<Result<(), PartyError>>;

    /// Whether the session survives a restart.
    fn session_is_durable(&self) -> bool;

    /// Where the session is being kept, for the welcome screen.
    fn store_description(&self) -> String;
}

/// How long the browser has to finish before the attempt is abandoned.
const SIGN_IN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// The real platform: OAuth in the browser, tokens in the keychain, the PartyTime API.
pub struct HttpPlatform {
    oauth: Arc<OAuthClient>,
    api: Arc<ApiClient>,
    config: ClientConfig,
    origin: String,
}

impl HttpPlatform {
    /// Builds the platform for a launch.
    ///
    /// `http` is GPUI's client, which is the one that already knows about the machine's
    /// proxies and TLS.
    pub fn new(
        http: Arc<dyn gpui_kit::http_client::HttpClient>,
        origin: impl Into<String>,
    ) -> Self {
        let origin: String = origin.into();
        let config = ClientConfig::from_env();
        let store = default_store(format!("oauth:{}", config.client_id));
        let oauth = Arc::new(OAuthClient::new(
            Arc::clone(&http),
            origin.clone(),
            config.clone(),
            store,
        ));
        let api = Arc::new(ApiClient::new(http, origin.clone(), Arc::clone(&oauth)));
        Self {
            oauth,
            api,
            config,
            origin,
        }
    }

    /// This build's OAuth configuration.
    #[must_use]
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// The platform origin.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The API client, for callers that already hold a session.
    #[must_use]
    pub fn api(&self) -> &Arc<ApiClient> {
        &self.api
    }

    /// Picks up a session stored by a previous launch.
    ///
    /// A stored refresh token is exchanged for a fresh pair; the access token is never
    /// stored, so there is nothing older than that to reuse.
    pub async fn restore(&self) -> Result<Option<TokenSet>, PartyError> {
        self.oauth.restore().await.map_err(PartyError::from)
    }

    /// Adopts a token pair, so the API client starts using it.
    pub fn adopt(&self, tokens: &TokenSet) {
        self.api.set_tokens(tokens.clone());
    }
}

impl Platform for HttpPlatform {
    fn start_sign_in(&self) -> PlatformFuture<Result<TokenSet, PartyError>> {
        let oauth = Arc::clone(&self.oauth);
        let api = Arc::clone(&self.api);
        Box::pin(async move {
            let endpoints = oauth.endpoints().await.map_err(PartyError::from)?;
            let pkce = oauth.begin();
            let state = studio_party::pkce::state();

            // Bound the port before opening anything: a redirect URI that cannot be served
            // should fail before the user has typed a password anywhere.
            let listener = CallbackListener::bind(&oauth.config().redirect_uri)
                .map_err(|error| PartyError::SignInRefused(error.to_string()))?;

            let url = studio_party::oauth::authorize_url(
                &endpoints,
                oauth.config(),
                &pkce.challenge,
                state.as_str(),
            );
            studio_party::oauth::open_browser(&url)
                .map_err(|error| PartyError::Transport(error.to_string()))?;

            let code =
                listener
                    .wait(state.as_str(), SIGN_IN_TIMEOUT)
                    .map_err(|error| match error {
                        studio_party::oauth::CallbackError::Denied => PartyError::SignInDenied,
                        other => PartyError::SignInRefused(other.to_string()),
                    })?;

            let tokens = oauth
                .exchange(&code, &pkce)
                .await
                .map_err(PartyError::from)?;
            api.set_tokens(tokens.clone());
            Ok(tokens)
        })
    }

    fn me(&self) -> PlatformFuture<Result<Me, PartyError>> {
        let api = Arc::clone(&self.api);
        Box::pin(async move { api.me().await })
    }

    fn parties(&self, live_only: bool) -> PlatformFuture<Result<PartyList, PartyError>> {
        let api = Arc::clone(&self.api);
        Box::pin(async move { api.parties(live_only).await })
    }

    fn party(&self, id: String) -> PlatformFuture<Result<PartyDetail, PartyError>> {
        let api = Arc::clone(&self.api);
        Box::pin(async move { api.party(&id).await })
    }

    fn sign_out(&self) -> PlatformFuture<Result<(), PartyError>> {
        let oauth = Arc::clone(&self.oauth);
        let api = Arc::clone(&self.api);
        Box::pin(async move {
            let result = match api.tokens() {
                Some(tokens) => oauth.revoke(&tokens).await.map_err(PartyError::from),
                None => {
                    // Nothing to revoke, but the stored secret still goes.
                    let _ = oauth.store().clear();
                    Ok(())
                }
            };
            api.clear_tokens();
            result
        })
    }

    fn session_is_durable(&self) -> bool {
        self.oauth.store().is_durable()
    }

    fn store_description(&self) -> String {
        self.oauth.store().description().to_string()
    }
}
