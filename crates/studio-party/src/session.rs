//! The console's session: who is signed in, and which profile and party are chosen.
//!
//! Pure state with no I/O, so the onboarding flow is testable without a browser. The
//! network half lives in [`crate::oauth`] and [`crate::api`].
//!
//! PartyTime holds no password at any point. The user types it into their browser, on the
//! platform's page; the console only ever holds tokens.

use crate::error::PartyError;
use crate::models::Profile;
use crate::token::TokenSet;

/// Who is signed in.
///
/// The handle is what the interface shows. There is no email here, because the platform
/// never sends one: an app token does not carry addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// User id, bare, exactly as the platform sent it.
    pub user_id: String,
    /// Public handle.
    pub handle: String,
    /// Chosen display name, when they set one.
    pub display_name: String,
}

impl Identity {
    /// What to call the user: the display name if there is one, otherwise the handle.
    #[must_use]
    pub fn label(&self) -> &str {
        if self.display_name.is_empty() {
            &self.handle
        } else {
            &self.display_name
        }
    }
}

impl From<&Profile> for Identity {
    fn from(profile: &Profile) -> Self {
        Self {
            user_id: profile.id.clone(),
            handle: profile.handle.clone(),
            display_name: profile.display_name.clone(),
        }
    }
}

/// How far a sign-in attempt has got, so the welcome screen can say what it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInStage {
    /// Preparing the request and opening the browser.
    Starting,
    /// The browser is open; waiting for the user to consent.
    WaitingForBrowser,
    /// Exchanging the code for tokens.
    Exchanging,
}

impl SignInStage {
    /// The line the welcome screen shows while this stage is current.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Starting => "Opening your browser…",
            Self::WaitingForBrowser => "Finish signing in, then come back here.",
            Self::Exchanging => "Finishing sign-in…",
        }
    }
}

/// Where the sign-in stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    /// Nobody is signed in.
    SignedOut,
    /// A sign-in is in flight. Every control that would start one is disabled.
    SigningIn(SignInStage),
    /// Signed in, holding a token pair.
    SignedIn {
        /// Who is signed in.
        identity: Identity,
        /// The current tokens.
        tokens: TokenSet,
    },
    /// The last attempt failed.
    Failed {
        /// The refusal, with its human copy.
        error: PartyError,
    },
}

impl AuthState {
    /// The identity, when signed in.
    #[must_use]
    pub fn identity(&self) -> Option<&Identity> {
        match self {
            Self::SignedIn { identity, .. } => Some(identity),
            _ => None,
        }
    }

    /// The tokens, when signed in.
    #[must_use]
    pub fn tokens(&self) -> Option<&TokenSet> {
        match self {
            Self::SignedIn { tokens, .. } => Some(tokens),
            _ => None,
        }
    }

    /// Whether a sign-in is in flight.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::SigningIn(_))
    }

    /// Whether a session exists.
    #[must_use]
    pub const fn is_signed_in(&self) -> bool {
        matches!(self, Self::SignedIn { .. })
    }
}

/// What the user has chosen during onboarding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Onboarding {
    /// The profile to load when the console opens the producer view.
    pub profile: Option<String>,
    /// The party to publish into, exactly as the platform sent its id.
    pub party: Option<String>,
}

impl Onboarding {
    /// Whether onboarding is finished and the producer view may open.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.profile.is_some() && self.party.is_some()
    }
}

/// The whole console session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSession {
    origin: String,
    auth: AuthState,
    onboarding: Onboarding,
}

impl ConsoleSession {
    /// A session against `origin`, signed out.
    #[must_use]
    pub fn new(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            auth: AuthState::SignedOut,
            onboarding: Onboarding::default(),
        }
    }

    /// The platform origin every request is sent to.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The current sign-in state.
    #[must_use]
    pub const fn auth(&self) -> &AuthState {
        &self.auth
    }

    /// What the user has chosen during onboarding.
    #[must_use]
    pub const fn onboarding(&self) -> &Onboarding {
        &self.onboarding
    }

    /// Whether the console should skip onboarding and open the producer view.
    #[must_use]
    pub const fn is_bootstrapped(&self) -> bool {
        self.auth.is_signed_in() && self.onboarding.is_complete()
    }

    /// The onboarding step on screen, counting from one. The welcome screen is step one.
    #[must_use]
    pub fn step(&self) -> usize {
        if !self.auth.is_signed_in() {
            1
        } else if self.onboarding.profile.is_none() {
            2
        } else if self.onboarding.party.is_none() {
            3
        } else {
            4
        }
    }

    /// Marks a sign-in as started. Ignored when one is already in flight.
    pub fn begin_sign_in(&mut self, stage: SignInStage) {
        if !self.auth.is_busy() {
            self.auth = AuthState::SigningIn(stage);
        }
    }

    /// Moves a sign-in to its next stage.
    pub fn advance_sign_in(&mut self, stage: SignInStage) {
        if self.auth.is_busy() {
            self.auth = AuthState::SigningIn(stage);
        }
    }

    /// Records a completed sign-in and resets onboarding choices, because they belonged to
    /// the previous user.
    pub fn signed_in(&mut self, identity: Identity, tokens: TokenSet) {
        self.auth = AuthState::SignedIn { identity, tokens };
        self.onboarding = Onboarding::default();
    }

    /// Adopts fresh tokens without disturbing the rest of the session.
    pub fn tokens_refreshed(&mut self, tokens: TokenSet) {
        if let AuthState::SignedIn { identity, .. } = &self.auth {
            let identity = identity.clone();
            self.auth = AuthState::SignedIn { identity, tokens };
        }
    }

    /// Records a refused sign-in.
    pub fn sign_in_failed(&mut self, error: PartyError) {
        self.auth = AuthState::Failed { error };
    }

    /// Clears the session entirely.
    pub fn sign_out(&mut self) {
        self.auth = AuthState::SignedOut;
        self.onboarding = Onboarding::default();
    }

    /// Records the profile the console will load.
    pub fn choose_profile(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.onboarding.profile = (!name.is_empty()).then_some(name);
    }

    /// Records the party to publish into, exactly as the platform sent its id.
    pub fn choose_party(&mut self, id: impl Into<String>) {
        let id = id.into();
        self.onboarding.party = (!id.is_empty()).then_some(id);
    }

    /// Forgets the chosen party.
    ///
    /// Stepping back has to clear this as well as the view's own copy: the step number
    /// the onboarding screen renders comes from here, so clearing one of the two leaves
    /// the screen showing a step its contents disagree with.
    pub fn clear_party(&mut self) {
        self.onboarding.party = None;
    }

    /// Forgets the chosen profile. See [`Self::clear_party`].
    pub fn clear_profile(&mut self) {
        self.onboarding.profile = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::PublishKind;
    use crate::token::TokenSet;
    use std::time::{Duration, SystemTime};

    fn identity() -> Identity {
        Identity {
            user_id: "i5g9".into(),
            handle: "ada".into(),
            display_name: "Ada".into(),
        }
    }

    fn tokens() -> TokenSet {
        TokenSet::from_parts(
            "pt_access".into(),
            "pr_refresh".into(),
            Duration::from_secs(3600),
            "profile:read publish".into(),
            SystemTime::now(),
        )
    }

    fn ready_session() -> ConsoleSession {
        let mut session = ConsoleSession::new("https://openparty.example");
        session.signed_in(identity(), tokens());
        session.choose_profile("Friday Night");
        session.choose_party("wlyayz1ytl2u822bifb4");
        session
    }

    #[test]
    fn a_fresh_session_is_not_bootstrapped() {
        assert!(!ConsoleSession::new("https://o").is_bootstrapped());
    }

    #[test]
    fn bootstrapping_requires_a_sign_in_a_profile_and_a_party() {
        let mut session = ConsoleSession::new("https://o");
        session.signed_in(identity(), tokens());
        assert!(!session.is_bootstrapped(), "signed in but nothing chosen");

        session.choose_profile("Default");
        assert!(!session.is_bootstrapped(), "no party yet");

        session.choose_party("p1");
        assert!(session.is_bootstrapped());
    }

    #[test]
    fn signing_in_resets_previous_choices_because_they_belonged_to_the_old_user() {
        let mut session = ready_session();
        session.signed_in(identity(), tokens());
        assert_eq!(session.onboarding().profile, None);
        assert_eq!(session.onboarding().party, None);
    }

    #[test]
    fn the_welcome_screen_is_the_first_step_and_the_sign_in_stage_is_visible() {
        let mut session = ConsoleSession::new("https://o");
        assert_eq!(session.step(), 1);

        session.begin_sign_in(SignInStage::Starting);
        assert!(session.auth().is_busy());
        assert_eq!(
            session.step(),
            1,
            "still the welcome screen while the browser is open"
        );
        assert!(!SignInStage::WaitingForBrowser.message().is_empty());
        assert!(!SignInStage::Exchanging.message().is_empty());
    }

    #[test]
    fn a_second_sign_in_while_one_is_in_flight_does_not_restart_it() {
        let mut session = ConsoleSession::new("https://o");
        session.begin_sign_in(SignInStage::Starting);
        session.begin_sign_in(SignInStage::Exchanging);
        assert_eq!(session.auth(), &AuthState::SigningIn(SignInStage::Starting));
    }

    #[test]
    fn advancing_only_matters_while_a_sign_in_is_in_flight() {
        let mut session = ConsoleSession::new("https://o");
        session.begin_sign_in(SignInStage::Starting);
        session.advance_sign_in(SignInStage::WaitingForBrowser);
        assert_eq!(
            session.auth(),
            &AuthState::SigningIn(SignInStage::WaitingForBrowser)
        );

        // Once it has failed, a late advance cannot revive it.
        session.sign_in_failed(PartyError::SignInDenied);
        session.advance_sign_in(SignInStage::Exchanging);
        assert!(!session.auth().is_busy());
    }

    #[test]
    fn a_failed_sign_in_keeps_the_refusal_and_is_not_busy() {
        let mut session = ConsoleSession::new("https://o");
        session.sign_in_failed(PartyError::SignInDenied);
        assert!(!session.auth().is_busy());
        assert!(!session.is_bootstrapped());
        let AuthState::Failed { error } = session.auth() else {
            panic!("expected Failed, got {:?}", session.auth());
        };
        assert_eq!(error.server_text(), "access_denied");
    }

    #[test]
    fn refreshing_tokens_does_not_disturb_the_rest_of_the_session() {
        let mut session = ready_session();
        let fresh = TokenSet::from_parts(
            "pt_new".into(),
            "pr_new".into(),
            Duration::from_secs(3600),
            "profile:read publish".into(),
            SystemTime::now(),
        );
        session.tokens_refreshed(fresh.clone());

        assert_eq!(session.auth().identity(), Some(&identity()));
        assert_eq!(session.auth().tokens(), Some(&fresh));
        assert_eq!(
            session.onboarding().profile.as_deref(),
            Some("Friday Night")
        );
    }

    #[test]
    fn signing_out_clears_everything() {
        let mut session = ready_session();
        session.sign_out();
        assert!(!session.is_bootstrapped());
        assert_eq!(session.auth(), &AuthState::SignedOut);
        assert_eq!(session.onboarding().party, None);
    }

    #[test]
    fn empty_choices_are_refused_rather_than_stored() {
        let mut session = ConsoleSession::new("https://o");
        session.choose_profile("");
        session.choose_party("");
        assert_eq!(session.onboarding().profile, None);
        assert_eq!(session.onboarding().party, None);
    }

    #[test]
    fn a_party_id_is_stored_exactly_as_received() {
        let mut session = ConsoleSession::new("https://o");
        session.choose_party("wlyayz1ytl2u822bifb4");
        assert_eq!(
            session.onboarding().party.as_deref(),
            Some("wlyayz1ytl2u822bifb4")
        );
    }

    #[test]
    fn the_interface_shows_the_handle_not_an_email() {
        let named = Identity {
            user_id: "u".into(),
            handle: "ada".into(),
            display_name: "Ada".into(),
        };
        assert_eq!(named.label(), "Ada");
        let unnamed = Identity {
            user_id: "u".into(),
            handle: "ada".into(),
            display_name: String::new(),
        };
        assert_eq!(unnamed.label(), "ada");
        assert!(!unnamed.label().contains('@'));
    }

    #[test]
    fn an_identity_comes_from_the_platform_profile_verbatim() {
        let profile = Profile {
            id: "i5g9espl8obrv10kdm9o".into(),
            handle: "partytimesmoke".into(),
            display_name: "PartyTime Smoke".into(),
            avatar_url: None,
            banner_url: None,
            bio: None,
            links: Vec::new(),
        };
        let identity = Identity::from(&profile);
        assert_eq!(identity.user_id, "i5g9espl8obrv10kdm9o");
        assert_eq!(identity.handle, "partytimesmoke");
    }

    #[test]
    fn a_kind_still_crosses_the_crate_boundary_unchanged() {
        // A guard against the kind vocabulary drifting out of this crate by accident.
        assert_eq!(PublishKind::ALL.len(), 5);
    }
}
