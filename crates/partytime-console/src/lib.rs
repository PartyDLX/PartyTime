//! PartyTime — the OpenParty desktop publishing console.
//!
//! Three screens, in the order a creator meets them: [`splash`] while the console
//! starts, [`onboarding`] to sign in and choose a profile and a party, and
//! [`producer`] to publish. [`app`] owns the routing between them; [`paths`] owns
//! what the console remembers between launches.
//!
//! The crates below this one own the two things the console talks to: `partytime-api`
//! is the OpenParty client, and `partytime-engine` owns profiles and media state.

pub mod app;
pub mod appearance;
pub mod http;
pub mod menu;
pub mod onboarding;
pub mod paths;
pub mod platform;
pub mod producer;
pub mod splash;
pub mod theme;

pub use app::{AppShell, Route};
