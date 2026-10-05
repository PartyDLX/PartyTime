//! PartyTime — the OpenParty Studio publishing console.
//!
//! Three screens, in the order a creator meets them: [`splash`] while the console
//! starts, [`onboarding`] to sign in and choose a profile and a party, and
//! [`producer`] to publish. [`app`] owns the routing between them; [`paths`] owns
//! what the console remembers between launches.
//!
//! The crates below this one own the two things the console talks to: `studio-party`
//! is the OpenParty API, and `studio-engine` is the profile and the media engine.

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
