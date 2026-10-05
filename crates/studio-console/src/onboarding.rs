//! Onboarding: the welcome screen, a profile, and a party.
//!
//! Step one is the welcome screen. There is no password field, because PartyTime is an
//! OAuth public client: pressing **Sign in with OpenParty** opens the system browser, the
//! user signs in and consents on the platform's own page, and the console receives an
//! authorization code over a loopback redirect. The console never sees an OpenParty
//! password.
//!
//! Step three lists the parties this user can stream to. There is nothing to paste: the
//! platform knows which parties the signed-in user belongs to.

use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};
use studio_engine::{Profile, ProfileStore};
use studio_party::{ConsoleSession, Identity, Me, PartyError, PartySummary, TokenSet};

use crate::{appearance::AppearanceMenu, paths::Paths, platform::Platform, theme};

/// A failure shown above the form, with the platform's own sentence beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingError {
    /// What the creator should do.
    pub message: SharedString,
    /// The platform's verbatim sentence.
    pub server_text: SharedString,
    /// Whether trying again could work.
    pub retryable: bool,
}

impl From<PartyError> for OnboardingError {
    fn from(error: PartyError) -> Self {
        Self {
            message: error.to_string().into(),
            server_text: error.server_text().to_string().into(),
            retryable: error.is_retryable(),
        }
    }
}

/// The scopes the console asks for, and what each one is for.
///
/// Shown on the welcome screen rather than only on the platform's consent screen: the user
/// should know what they are agreeing to before they leave the app.
pub const SCOPE_PURPOSES: [(&str, &str); 4] = [
    ("profile:read", "Your handle and display name"),
    ("channels:read", "Channels you own or edit"),
    ("parties:read", "Parties you belong to, and their rosters"),
    ("publish", "Declare inputs, go live, and publish"),
];

/// The onboarding view.
pub struct OnboardingView {
    store: ProfileStore,
    session: ConsoleSession,
    platform: Entity<PlatformBox>,

    stage: Option<studio_party::SignInStage>,
    identity: Option<Identity>,
    scopes: Vec<String>,

    profile: Option<String>,
    parties: Vec<PartySummary>,
    live_only: bool,
    loading_parties: bool,
    selected_party: Option<String>,
    error: Option<OnboardingError>,
    finished: Option<OnboardingResult>,
    paths: Option<Paths>,
}

/// A retained handle to the platform, so a spawned task can reach it.
pub struct PlatformBox(pub Box<dyn Platform>);

/// The answers onboarding collects when it finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingResult {
    /// Profile to load.
    pub profile: String,
    /// Party to publish into, exactly as the platform sent its id.
    pub party: String,
    /// The account the console opens with.
    pub account: String,
    /// What the platform said about that party, so the console can show it without
    /// guessing: the console does not know a party is live until the API says so.
    pub party_summary: Option<PartySummary>,
}

impl OnboardingView {
    /// Creates the view.
    pub fn new(
        store: ProfileStore,
        session: ConsoleSession,
        platform: Box<dyn Platform>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            store,
            session,
            platform: cx.new(|_| PlatformBox(platform)),
            stage: None,
            identity: None,
            scopes: Vec::new(),
            profile: None,
            parties: Vec::new(),
            live_only: true,
            loading_parties: false,
            selected_party: None,
            error: None,
            finished: None,
            paths: None,
        }
    }

    /// Points the view at the console's directories, for profile import.
    pub fn set_paths(&mut self, paths: Paths) {
        self.paths = Some(paths);
    }

    /// Which step is on screen, one-based. The welcome screen is step one.
    #[must_use]
    pub fn step(&self) -> usize {
        self.session.step()
    }

    /// How many steps there are.
    #[must_use]
    pub const fn step_count() -> usize {
        4
    }

    /// The title of the current step.
    #[must_use]
    pub fn step_title(&self) -> &'static str {
        match self.step() {
            1 => "Welcome to PartyTime",
            2 => "Choose a profile",
            3 => "Choose a party",
            _ => "Ready",
        }
    }

    /// Whether a request is in flight, and which.
    #[must_use]
    pub const fn stage(&self) -> Option<studio_party::SignInStage> {
        self.stage
    }

    /// The error on screen, if any.
    #[must_use]
    pub fn error(&self) -> Option<&OnboardingError> {
        self.error.as_ref()
    }

    /// The signed-in identity, once there is one.
    #[must_use]
    pub fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    /// The scopes the token actually carries, once known.
    #[must_use]
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    /// Whether a session exists.
    #[must_use]
    pub fn session_is_signed_in(&self) -> bool {
        self.session.auth().is_signed_in()
    }

    /// The selected party id, exactly as the platform sent it.
    #[must_use]
    pub fn selected_party_id(&self) -> Option<String> {
        self.selected_party.clone()
    }

    /// The profile chosen so far, if any.
    #[must_use]
    pub fn chosen_profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    /// The parties offered for streaming, live ones first.
    #[must_use]
    pub fn parties(&self) -> &[PartySummary] {
        &self.parties
    }

    /// Whether the party list is filtered to live parties.
    #[must_use]
    pub const fn live_only(&self) -> bool {
        self.live_only
    }

    /// The answers, once onboarding is complete.
    #[must_use]
    pub fn result(&self) -> Option<&OnboardingResult> {
        self.finished.as_ref()
    }

    /// Whether the console may open.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.profile.is_some() && self.selected_party.is_some() && self.identity.is_some()
    }

    /// Runs the browser sign-in, then loads the party list.
    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.stage.is_some() {
            return;
        }
        self.error = None;
        self.stage = Some(studio_party::SignInStage::Starting);
        let client = self.platform.clone();

        cx.spawn(async move |this, cx| {
            let tokens: Result<TokenSet, PartyError> = client
                .update(cx, |client, _| client.0.start_sign_in())
                .await;
            this.update(cx, |this, cx| {
                match tokens {
                    Ok(tokens) => {
                        this.session.signed_in(
                            this.identity.clone().unwrap_or_else(|| Identity {
                                user_id: String::new(),
                                handle: String::new(),
                                display_name: String::new(),
                            }),
                            tokens,
                        );
                        // The token is in; fetch the party list straight away rather than
                        // making the user ask for it.
                        this.stage = None;
                        this.load_parties(cx);
                    }
                    Err(error) => {
                        this.stage = None;
                        this.error = Some(error.into());
                    }
                }
                // The state above changed; without this the window keeps showing
                // "Opening your browser…" until something else happens to repaint.
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Records who is signed in and the scopes the token carries.
    pub fn signed_in(&mut self, me: &Me) {
        self.identity = Some(Identity::from(&me.profile));
        self.scopes = me.scopes.clone();
        self.stage = None;
        self.error = None;
        // A console may be granted fewer scopes than it asked for. Say which, rather than
        // failing later at a 403 the user cannot connect to a choice they made.
        if let Some(missing) = SCOPE_PURPOSES
            .iter()
            .map(|(scope, _)| *scope)
            .find(|scope| !me.has_scope(scope))
        {
            self.error = Some(OnboardingError {
                message: format!("PartyTime was not granted {missing}.").into(),
                server_text: "insufficient_scope".into(),
                retryable: false,
            });
        }
    }

    /// Records a sign-in failure.
    pub fn sign_in_failed(&mut self, error: PartyError) {
        self.stage = None;
        self.error = Some(error.into());
    }

    /// Loads the parties this user can stream to.
    pub fn load_parties(&mut self, cx: &mut Context<Self>) {
        if self.loading_parties {
            return;
        }
        self.loading_parties = true;
        let live_only = self.live_only;
        let client = self.platform.clone();
        cx.spawn(async move |this, cx| {
            let me = client.update(cx, |client, _| client.0.me()).await;
            let parties = client
                .update(cx, |client, _| client.0.parties(live_only))
                .await;
            this.update(cx, |this, cx| {
                this.loading_parties = false;
                if let Ok(me) = me {
                    this.signed_in(&me);
                }
                match parties {
                    Ok(list) => {
                        this.clear_error();
                        this.parties = list.parties;
                    }
                    Err(error) => this.error = Some(error.into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Filters the party list to live parties, or shows every party.
    pub fn set_live_only(&mut self, live_only: bool, cx: &mut Context<Self>) {
        if self.live_only != live_only {
            self.live_only = live_only;
            cx.notify();
        }
    }

    /// Records the profile to load, creating a starter profile when the store is empty.
    pub fn choose_profile(&mut self, name: impl Into<String>, cx: &mut Context<Self>) {
        self.profile = Some(name.into());
        self.session
            .choose_profile(self.profile.clone().unwrap_or_default());
        cx.notify();
    }

    /// Creates and selects a starter profile, for a creator with no OBS config yet.
    pub fn create_starter_profile(&mut self, name: &str, cx: &mut Context<Self>) {
        let mut store = self.store.clone();
        let profile = Profile::starter(name);
        if let Err(error) = store.save(&profile) {
            self.error = Some(OnboardingError {
                message: error.to_string().into(),
                server_text: "".into(),
                retryable: false,
            });
            cx.notify();
            return;
        }
        self.choose_profile(profile.name, cx);
    }

    /// The profiles available to choose from.
    pub fn available_profiles(&self) -> Vec<studio_engine::store::ProfileSummary> {
        self.store.list().unwrap_or_default()
    }

    /// Selects a party by the id the platform sent.
    pub fn choose_party(&mut self, id: impl Into<String>, cx: &mut Context<Self>) {
        self.selected_party = Some(id.into());
        self.session
            .choose_party(self.selected_party.clone().unwrap_or_default());
        cx.notify();
    }

    /// Forgets the chosen profile, returning the screen to that step.
    pub fn clear_profile(&mut self, cx: &mut Context<Self>) {
        self.profile = None;
        self.session.clear_profile();
        cx.notify();
    }

    /// Steps back one stage.
    ///
    /// Both the view's copy and the session's are cleared: the step number this screen
    /// renders comes from the session, so clearing only one leaves it stuck.
    pub fn step_back(&mut self, cx: &mut Context<Self>) {
        match self.step() {
            4 => {
                self.selected_party = None;
                self.session.clear_party();
            }
            3 => {
                self.profile = None;
                self.session.clear_profile();
            }
            _ => return,
        }
        cx.notify();
    }

    /// Finishes onboarding, producing the answers the shell needs.
    pub fn finish(&mut self, cx: &mut Context<Self>) {
        let (Some(profile), Some(party)) = (self.profile.clone(), self.selected_party.clone())
        else {
            return;
        };
        let account = self
            .identity
            .as_ref()
            .map_or_else(String::new, |i| i.handle.clone());
        self.session.choose_profile(profile.clone());
        let party_summary = self
            .parties
            .iter()
            .find(|candidate| Some(candidate.id.as_str()) == Some(party.as_str()))
            .cloned();
        self.finished = Some(OnboardingResult {
            profile,
            party,
            account,
            party_summary,
        });
        cx.notify();
    }

    /// Clears the error, for when the user changes the thing that caused it.
    fn clear_error(&mut self) {
        self.error = None;
    }
}

impl Render for OnboardingView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let step = self.step();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .child(
                v_flex()
                    .w(px(460.))
                    .gap_4()
                    .p_6()
                    .rounded_lg()
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(step_header(step, Self::step_count(), self.step_title(), cx))
                    .child(match step {
                        1 => self.render_welcome(cx),
                        2 => self.render_profile(cx),
                        3 => self.render_party(cx),
                        _ => self.render_ready(cx),
                    })
                    .when_some(self.error.clone(), |this, error| {
                        this.child(error_block(&error, cx))
                    })
                    .child(self.render_footer(step, cx)),
            )
    }
}

impl OnboardingView {
    fn render_welcome(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.stage.is_some();
        let stage = self.stage;
        let durable = self.platform.read(cx).0.session_is_durable();
        let where_stored = self.platform.read(cx).0.store_description();

        v_flex()
            .gap_4()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("Publish to a party from this machine."),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Your browser handles sign-in. PartyTime never sees your OpenParty password."),
                    ),
            )
            .child(
                Button::new("signin")
                    .primary()
                    .w_full()
                    .icon(IconName::ExternalLink)
                    .label(if busy { "Signing in…" } else { "Sign in with OpenParty" })
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in(cx))),
            )
            .when_some(stage.map(|stage| stage.message()), |this, message| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(message.to_string()),
                )
            })
            .child(
                v_flex()
                    .id("scopes")
                    .test_support()
                    .gap_1()
                    .children(SCOPE_PURPOSES.map(|(scope, purpose)| {
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(IconName::Check).xsmall().text_color(cx.theme().success))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{scope} — {purpose}")),
                            )
                    })),
            )
            .when(!durable, |this| {
                // Say this before they sign in rather than letting them find out by being
                // signed out in the morning.
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .child(format!("No system credential store found: the session is kept in {where_stored}.")),
                )
            })
            .into_any_element()
    }

    fn render_profile(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let profiles = self.available_profiles();
        let body: AnyElement =
            if profiles.is_empty() {
                empty_note(
                    "No profiles yet",
                    "Start a new one, or import the scene collection OBS wrote.",
                    cx,
                )
            } else {
                v_flex()
                    .gap_1()
                    .children(profiles.iter().map(|summary| {
                        let name: SharedString = summary.name.clone().into();
                        let selected = self.profile.as_deref() == Some(name.as_str());
                        Button::new(format!("choose-profile:{name}"))
                            .w_full()
                            .when(selected, Button::primary)
                            .when(!selected, Button::ghost)
                            .label(format!(
                                "{} · {} scenes · {} inputs",
                                name, summary.scenes, summary.sources
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_profile(name.clone(), cx)
                            }))
                            .into_any_element()
                    }))
                    .into_any_element()
            };

        v_flex()
            .gap_3()
            .child(body)
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("profile-new")
                            .label("New profile")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.create_starter_profile("Default", cx);
                            })),
                    )
                    .child(
                        Button::new("profile-import")
                            .ghost()
                            .label("Import from OBS…")
                            .disabled(self.paths.is_none())
                            .tooltip("Choose an OBS scene collection JSON to bring your setup in")
                            .on_click(cx.listener(|_, _, _, _| {})),
                    ),
            )
            .into_any_element()
    }

    fn render_party(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let header = h_flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(match self.parties.len() {
                        0 => "No parties".to_string(),
                        1 => "1 party".to_string(),
                        n => format!("{n} parties"),
                    }),
            )
            .child(
                Button::new("party-live-only")
                    .ghost()
                    .xsmall()
                    .label("Live only")
                    .when(self.live_only, |this| this.primary())
                    .when(!self.live_only, |this| this.ghost())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let next = !this.live_only;
                        this.set_live_only(next, cx);
                        this.load_parties(cx);
                    })),
            );

        let body: AnyElement = if self.loading_parties {
            v_flex()
                .id("parties-loading")
                .items_center()
                .gap_2()
                .p_6()
                .child(gpui_kit::component::spinner::Spinner::new())
                .into_any_element()
        } else if self.parties.is_empty() {
            empty_note(
                "Nothing to stream to",
                "You are not on a party that has started. Ask for an invite in the web app.",
                cx,
            )
        } else {
            v_flex()
                .id("parties-list")
                .test_support()
                .gap_1()
                .children(self.parties.clone().into_iter().map(|party| {
                    let id = party.id.clone();
                    let selected = self.selected_party.as_deref() == Some(id.as_str());
                    Button::new(format!("party:{id}"))
                        .w_full()
                        .justify_start()
                        .when(selected, Button::primary)
                        .when(!selected, Button::ghost)
                        .label(party_row_label(&party))
                        .tooltip(party_row_tooltip(&party))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.choose_party(id.clone(), cx)),
                        )
                        .into_any_element()
                }))
                .into_any_element()
        };

        v_flex()
            .gap_2()
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn render_ready(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .parties
            .iter()
            .find(|party| Some(party.id.as_str()) == self.selected_party.as_deref())
            .map_or_else(
                || self.selected_party.clone().unwrap_or_default(),
                |party| party.title.clone(),
            );
        let approved: Vec<&str> = self
            .parties
            .iter()
            .find(|party| Some(party.id.as_str()) == self.selected_party.as_deref())
            .map(|party| {
                party
                    .approved_kinds
                    .iter()
                    .map(|kind| kind.label())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        v_flex()
            .gap_2()
            .child(div().text_sm().child(format!(
                "Profile: {}",
                self.profile.clone().unwrap_or_default()
            )))
            .child(div().text_sm().child(format!("Party: {title}")))
            .when(!approved.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Approved for you: {}", approved.join(", "))),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("OpenParty.tv stays open in your browser for the room."),
            )
            .into_any_element()
    }

    fn render_footer(&mut self, step: usize, cx: &mut Context<Self>) -> AnyElement {
        let can_finish = self.is_complete();
        let mut row = h_flex().justify_between().gap_2();
        if step > 1 {
            row = row.child(
                Button::new("back")
                    .ghost()
                    .small()
                    .icon(IconName::ArrowLeft)
                    .label("Back")
                    .on_click(cx.listener(|this, _, _, cx| this.step_back(cx))),
            );
        }
        if step >= 4 {
            row = row.child(
                // "Console" with a trailing arrow. `Button` lays out icon, then label,
                // then children, so the arrow goes in a child slot rather than in the
                // label text — and it stays a real icon rather than a glyph in a string.
                Button::new("finish")
                    .primary()
                    .small()
                    .label("Console")
                    .child(Icon::new(IconName::ArrowRight).xsmall())
                    .disabled(!can_finish)
                    .on_click(cx.listener(|this, _, _, cx| this.finish(cx))),
            );
        }
        row.into_any_element()
    }
}

/// A party row's label: the title, with the facts that differ between rows kept out of it.
fn party_row_label(party: &PartySummary) -> String {
    let mut label = party.title.clone();
    if let Some(game) = &party.game_name {
        label.push_str(&format!(" · {game}"));
    }
    label
}

/// A party row's tooltip: the state a row cannot show inline.
fn party_row_tooltip(party: &PartySummary) -> String {
    let mut facts = vec![format!("Role: {}", party.role), party.visibility.clone()];
    if party.is_live() {
        facts.push("Live now".to_string());
    }
    let pending = party
        .my_inputs
        .iter()
        .filter(|input| !input.is_approved())
        .count();
    if pending > 0 {
        facts.push(format!("{pending} awaiting owner"));
    }
    if !party.approved_kinds.is_empty() {
        let approved: Vec<&str> = party
            .approved_kinds
            .iter()
            .map(|kind| kind.label())
            .collect();
        facts.push(format!("Approved: {}", approved.join(", ")));
    }
    facts.join(" · ")
}

fn step_header(
    step: usize,
    total: usize,
    title: &str,
    cx: &mut Context<OnboardingView>,
) -> AnyElement {
    h_flex()
        .id("step-header")
        .test_support()
        .justify_between()
        .items_center()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Step {step} of {total}")),
                )
                .child(
                    AppearanceMenu {
                        preference: theme::preference(cx),
                    }
                    .into_any_element(),
                ),
        )
        .into_any_element()
}

fn error_block(error: &OnboardingError, cx: &mut Context<OnboardingView>) -> AnyElement {
    let icon = if error.retryable {
        IconName::RefreshCw
    } else {
        IconName::CircleAlert
    };
    let color = if error.retryable {
        cx.theme().warning
    } else {
        cx.theme().danger
    };
    v_flex()
        .id("onboarding-error")
        .test_support()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(color)
        .p_3()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(icon).small().text_color(color))
                .child(div().text_sm().child(error.message.clone())),
        )
        .when(!error.server_text.is_empty(), |this| {
            // The platform's own sentence, verbatim, so a creator can quote it and a
            // moderator can match it to a log line.
            this.child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(error.server_text.clone()),
            )
        })
        .into_any_element()
}

fn empty_note(title: &str, detail: &str, cx: &mut Context<OnboardingView>) -> AnyElement {
    v_flex()
        .items_center()
        .gap_1()
        .p_4()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(title.to_string()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(detail.to_string()),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_party::PublishKind;

    #[test]
    fn the_scopes_shown_match_the_scopes_requested() {
        let requested: Vec<&str> = studio_party::oauth::SCOPES.to_vec();
        let shown: Vec<&str> = SCOPE_PURPOSES.iter().map(|(scope, _)| *scope).collect();
        assert_eq!(
            requested, shown,
            "the welcome screen must describe the real request"
        );
    }

    #[test]
    fn every_scope_is_given_a_plain_purpose() {
        for (scope, purpose) in SCOPE_PURPOSES {
            assert!(!scope.is_empty());
            assert!(!purpose.is_empty(), "{scope} has no explanation");
        }
    }

    #[test]
    fn a_party_row_keeps_its_title_and_game_on_the_label() {
        let party = PartySummary {
            id: "p1".into(),
            title: "Friday Night".into(),
            visibility: "public".into(),
            status: "live".into(),
            game_name: Some("Helldivers 2".into()),
            channel_id: None,
            allow_rogue: false,
            role: studio_party::PartyRole::Member,
            is_director: false,
            can_go_live: false,
            session: None,
            my_inputs: Vec::new(),
            approved_kinds: Vec::new(),
            director_handle: None,
        };
        assert_eq!(party_row_label(&party), "Friday Night · Helldivers 2");
        let tooltip = party_row_tooltip(&party);
        assert!(tooltip.contains("public"));
        assert!(tooltip.contains("Member"));
    }

    #[test]
    fn a_party_tooltip_reports_what_the_row_cannot() {
        let party = PartySummary {
            id: "p1".into(),
            title: "Friday Night".into(),
            visibility: "public".into(),
            status: "live".into(),
            game_name: None,
            channel_id: None,
            allow_rogue: false,
            role: studio_party::PartyRole::Owner,
            is_director: false,
            can_go_live: true,
            session: Some(studio_party::PartySession {
                id: "s".into(),
                status: "live".into(),
                started_at: None,
            }),
            my_inputs: vec![studio_party::MyInput {
                kind: PublishKind::Camera,
                label: None,
                consent: studio_party::ConsentState::Pending,
            }],
            approved_kinds: vec![PublishKind::Mic],
            director_handle: None,
        };
        let tooltip = party_row_tooltip(&party);
        assert!(tooltip.contains("Live now"), "{tooltip}");
        assert!(tooltip.contains("1 awaiting owner"), "{tooltip}");
        assert!(tooltip.contains("Approved: Mic"), "{tooltip}");
    }

    #[test]
    fn an_error_carries_both_copies_and_knows_whether_to_retry() {
        let error = OnboardingError::from(PartyError::from_response(409, "No live session."));
        assert_eq!(error.server_text, "No live session.");
        assert!(!error.retryable);
        assert_ne!(error.message.as_ref(), "No live session.");
    }

    #[test]
    fn a_refusal_carries_no_platform_sentence_of_its_own() {
        let error = OnboardingError::from(PartyError::SignInDenied);
        assert_eq!(error.server_text, "access_denied");
        assert!(!error.retryable, "a denial is not worth retrying");
    }
}
