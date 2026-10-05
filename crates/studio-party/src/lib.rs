//! The OpenParty.tv contract, as PartyTime consumes it.
//!
//! PartyTime is a **public OAuth client**. It never asks for, stores or transmits an
//! OpenParty password: the user signs in in their browser on the platform's own page, and
//! the console receives an authorization code it exchanges for tokens using PKCE.
//!
//! * [`discovery`] — where the authorization server says its endpoints are.
//! * [`pkce`] — the verifier, challenge and `state` that bind a callback to its request.
//! * [`callback`] — the loopback listener the registered redirect points at.
//! * [`oauth`] — authorize, exchange, refresh, revoke.
//! * [`store`] — where the refresh token lives. The OS keychain, or memory.
//! * [`token`] — the pair, and when it needs replacing.
//! * [`api`] — the `/api/partytime/v1` client.
//! * [`models`] — the wire types. Ids are used exactly as sent; see that module.
//! * [`session`] — who is signed in and what they have chosen.
//! * [`error`] — the platform's refusals, and what the console says about them.

#![forbid(unsafe_code)]

pub mod api;
pub mod callback;
pub mod discovery;
pub mod error;
pub mod kind;
pub mod models;
pub mod oauth;
pub mod pkce;
pub mod session;
pub mod store;
pub mod token;

pub use api::{ApiClient, BASE_PATH};
pub use callback::CallbackListener;
pub use discovery::Endpoints;
pub use error::PartyError;
pub use kind::PublishKind;
pub use models::{
    Channel, ChannelList, ConsentState, Me, MyInput, PartyDetail, PartyList, PartyRole,
    PartySession, PartySummary, Presence, Profile, PublishGrant, RosterMember,
};
pub use oauth::{ClientConfig, OAuthClient, OAuthError};
pub use session::{AuthState, ConsoleSession, Identity, Onboarding, SignInStage};
pub use store::{KeyringStore, MemoryStore, SecretStore, default_store};
pub use token::TokenSet;
