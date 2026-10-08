//! Runs PartyTime's sign-in without the window, for local development.
//!
//!     cargo run -p partytime-console --example signin-probe -- http://127.0.0.1:5199
//!
//! It performs exactly what the **Sign in with OpenParty** button performs — discovery,
//! PKCE, the loopback listener, the browser, the token exchange, `/me`, `/parties` and a
//! refresh — printing each step. That makes the flow exercisable on a machine with no
//! display and no OpenParty stack; `scripts/dev-oauth-stub.py` provides one.
//!
//! It deliberately uses the console's own HTTP transport rather than a stand-in, so a
//! pass here says something about the transport the application actually runs with.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use gpui_kit::http_client::HttpClient;
use partytime_api::oauth::CallbackError;
use partytime_api::store::{KeyringStore, MemoryStore, SecretStore};
use partytime_api::{ApiClient, CallbackListener, ClientConfig, OAuthClient};

/// A short, non-revealing prefix of a token, so the transcript can show rotation.
fn glimpse(token: &str) -> String {
    let head: String = token.chars().take(12).collect();
    format!("{head}…")
}

fn main() -> Result<()> {
    futures::executor::block_on(run())
}

async fn run() -> Result<()> {
    let origin = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:5199".to_string())
        .trim_end_matches('/')
        .to_string();
    let http: Arc<dyn HttpClient> = partytime_console::http::transport();

    let config = ClientConfig::from_env();
    // Falls back the same way the app does, so the probe runs on a machine with no
    // credential store.
    let store: Arc<dyn SecretStore> = if KeyringStore::is_available() {
        println!("store        : the system credential store");
        Arc::new(KeyringStore::new(format!("oauth:{}", config.client_id)))
    } else {
        println!("store        : memory only — no credential store on this machine");
        Arc::new(MemoryStore::new())
    };

    let oauth = Arc::new(OAuthClient::new(
        Arc::clone(&http),
        origin.clone(),
        config.clone(),
        store.clone(),
    ));
    let api = ApiClient::new(http, origin.clone(), Arc::clone(&oauth));

    println!("origin       : {origin}");
    println!("client id    : {}", config.client_id);
    println!("redirect uri : {}", config.redirect_uri);

    println!("\n1. discovery  : /.well-known/oauth-authorization-server");
    let endpoints = oauth.endpoints().await?;
    println!("   authorize  : {}", endpoints.authorization_endpoint);
    println!("   token      : {}", endpoints.token_endpoint);

    println!("\n2. loopback   : binding {}", config.redirect_uri);
    let listener =
        CallbackListener::bind(&config.redirect_uri).map_err(|error| anyhow!("{error}"))?;
    println!("   listening  : {}", listener.address());

    let pkce = oauth.begin();
    let state = partytime_api::pkce::state();
    let url =
        partytime_api::oauth::authorize_url(&endpoints, &config, &pkce.challenge, state.as_str());
    println!(
        "\n3. authorize  :\n   {url}\n\n   Opening your browser — press Allow to continue, or Deny to see the refusal path."
    );
    partytime_api::oauth::open_browser(&url).map_err(|why| anyhow!("{why}"))?;

    println!("\n4. callback   : waiting on {}", listener.address());
    let code = match listener.wait(state.as_str(), Duration::from_secs(300)) {
        Ok(code) => code,
        Err(CallbackError::Denied) => {
            println!("   declined (access_denied) — the console would show that and stop here.");
            return Ok(());
        }
        Err(error) => return Err(anyhow!("{error}")),
    };
    println!("   code        : {}", glimpse(&code));

    println!("\n5. token      : POST /oauth/token (grant_type=authorization_code)");
    let tokens = oauth.exchange(&code, &pkce).await?;
    println!("   access      : {}", glimpse(tokens.access_token()));
    println!("   refresh     : stored — {}", store.description());
    println!("   scope       : {}", tokens.scope());
    api.set_tokens(tokens.clone());

    println!("\n6. identity   : GET /api/partytime/v1/me");
    let me = api.me().await?;
    println!("   {} ({})", me.profile.display_name, me.profile.handle);
    println!(
        "   client      : {} · scopes {}",
        me.client_id,
        me.scopes.join(" ")
    );

    println!("\n7. parties    : GET /api/partytime/v1/parties?live=1");
    let parties = api.parties(true).await?;
    if parties.parties.is_empty() {
        println!("   (none)");
    }
    for party in &parties.parties {
        let approved: Vec<&str> = party
            .approved_kinds
            .iter()
            .map(|kind| kind.label())
            .collect();
        let pending = party
            .my_inputs
            .iter()
            .filter(|input| !input.is_approved())
            .count();
        println!(
            "   {} · {} · role {} · approved [{}]{}",
            party.id,
            party.title,
            party.role,
            approved.join(", "),
            if pending > 0 {
                format!(" · {pending} awaiting owner")
            } else {
                String::new()
            }
        );
    }

    println!("\n8. refresh    : rotating the refresh token");
    let rotated = api.refresh().await?;
    println!("   new access  : {}", glimpse(rotated.access_token()));

    println!("\nSign-in probe completed.");
    Ok(())
}
