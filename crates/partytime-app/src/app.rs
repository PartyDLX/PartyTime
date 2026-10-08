//! The application shell: bootstrap, routing, and the cross-fade between screens.
//!
//! The shell owns everything that outlives a screen — configuration, the profile
//! store, the session, the engine status — and hands each screen the state it needs.
//! Screens do not talk to each other.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
};
use partytime_api::ConsoleSession;
use partytime_engine::{EngineStatus, Profile, ProfileStore};

use crate::{
    menu::{
        AboutPartyTime, AppearanceDark, AppearanceLight, AppearanceSystem, ExportProfile,
        ImportObsProfile, Quit, Redo, SwitchProfile, ToggleLeftDock, ToggleRightDock, Undo,
    },
    onboarding::OnboardingView,
    paths::{ConsoleConfig, Paths, PathsError},
    platform::HttpPlatform,
    producer::{ProducerContext, ProducerView},
    splash::{BootStep, Splash, bootstrap_complete, first_failure},
    theme,
};

/// Which File command opened the path dialog.
///
/// The dialog is shared: it is one text field and a Confirm, and only this says whether
/// the field is a collection to read or a destination to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileDialog {
    Import,
    Export,
}

impl FileDialog {
    const fn title(self) -> &'static str {
        match self {
            Self::Import => "Import OBS profile…",
            Self::Export => "Export profile…",
        }
    }

    const fn blurb(self) -> &'static str {
        match self {
            Self::Import => "Path to an OBS scene collection.",
            Self::Export => "Where to write the current profile as an OBS scene collection.",
        }
    }

    const fn confirm_label(self) -> &'static str {
        match self {
            Self::Import => "Import",
            Self::Export => "Export",
        }
    }
}

/// A centred panel over the shell's own content.
///
/// Modal in appearance — it sits above the console rather than in the flow of it — while
/// staying inside the window's render, so it can be hit-tested and observed the way a
/// control anywhere else in the console can.
fn modal(panel: impl IntoElement) -> AnyElement {
    v_flex()
        .id("modal")
        .flex_1()
        .items_center()
        .justify_center()
        .child(panel)
        .into_any_element()
}

/// Where the console is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Bringing the console up.
    Splash,
    /// Signing in and choosing a profile and a party.
    Onboarding,
    /// The publishing console.
    Producer,
}

impl Route {
    /// A stable identity for the route, used to replay the fade when it changes.
    const fn index(self) -> usize {
        match self {
            Self::Splash => 0,
            Self::Onboarding => 1,
            Self::Producer => 2,
        }
    }
}

/// The shell.
pub struct AppShell {
    route: Route,
    paths: Option<Paths>,
    paths_error: Option<SharedString>,
    config: ConsoleConfig,
    store: ProfileStore,
    session: ConsoleSession,
    engine: EngineStatus,
    profile: Option<Profile>,
    steps: Vec<BootStep>,
    _appearance: Subscription,
    _onboarding_result: Option<Subscription>,
    onboarding_completed: bool,
    /// Makes the window root reachable by a dispatched action.
    focus: gpui_kit::FocusHandle,
    party_summary: Option<partytime_api::PartySummary>,
    onboarding: Option<Entity<OnboardingView>>,
    producer: Option<Entity<ProducerView>>,
    /// Which File command is waiting on the path dialog, if one is open.
    file_dialog: Option<FileDialog>,
    /// The dialog's text field, kept so it is not dropped while the dialog is up.
    path_input: Option<Entity<InputState>>,
    /// What the last File command could not do, shown verbatim rather than swallowed.
    file_error: Option<SharedString>,
    /// Keeps the dialog's Enter-to-confirm watch alive while the dialog is up.
    file_watch: Option<Subscription>,
    /// Whether Help -> About is showing.
    about_open: bool,
}

impl AppShell {
    /// Builds the shell and starts the bootstrap.
    ///
    /// The paths are resolved synchronously because a failure there is fatal and a
    /// single environment lookup is not worth a frame of work.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_at(Paths::resolve(), window, cx)
    }

    /// Builds the shell against an explicit configuration directory.
    ///
    /// Production resolves the paths from the environment; tests name them, so they
    /// never have to mutate process-wide state to run in parallel.
    pub fn new_at(
        resolved: Result<Paths, PathsError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_at_with_platform(resolved, None, window, cx)
    }

    /// Builds the shell against an explicit platform.
    ///
    /// Production uses [`Self::new_at`]; this seam exists so the whole onboarding
    /// journey — including the press that leaves it — can be driven without a server.
    pub fn new_at_with_platform(
        resolved: Result<Paths, PathsError>,
        platform: Option<Box<dyn crate::platform::Platform>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (paths, paths_error) = match &resolved {
            Ok(paths) => (Some(paths.clone()), None),
            Err(error) => (None, Some(error.to_string().into())),
        };
        let config = paths
            .as_ref()
            .map_or_else(ConsoleConfig::default, |paths: &Paths| {
                ConsoleConfig::load(&paths.settings_file())
            });

        // Built from the loaded settings, so the origin has exactly one source.
        let platform: Box<dyn crate::platform::Platform> = platform.unwrap_or_else(|| {
            Box::new(HttpPlatform::new(cx.http_client(), config.origin.clone()))
        });

        let store = paths.as_ref().map_or_else(
            || ProfileStore::new(std::env::temp_dir()),
            |paths: &Paths| ProfileStore::new(&paths.profiles_dir),
        );
        let session = ConsoleSession::new(config.origin.clone());

        let store_for_screen = store.clone();
        let session_for_screen = session.clone();

        let mut shell = Self {
            route: Route::Splash,
            paths,
            paths_error,
            config,
            store,
            session,
            engine: EngineStatus::default(),
            profile: None,
            steps: Vec::new(),
            _appearance: window.observe_window_appearance(theme::sync),
            _onboarding_result: None,
            onboarding_completed: false,
            focus: cx.focus_handle(),
            party_summary: None,
            onboarding: None,
            producer: None,
            file_dialog: None,
            path_input: None,
            file_error: None,
            file_watch: None,
            about_open: false,
        };

        shell.steps = vec![
            BootStep::running("Read configuration"),
            BootStep::pending("Load profile"),
            BootStep::pending("Start media engine"),
        ];
        shell.onboarding = Some(cx.new(|cx| {
            OnboardingView::new(
                store_for_screen.clone(),
                session_for_screen.clone(),
                platform,
                window,
                cx,
            )
        }));
        if let (Some(view), Some(paths)) = (&shell.onboarding, shell.paths.clone()) {
            view.update(cx, |view, _| view.set_paths(paths));
        }

        // The onboarding screen records a result; something has to notice and route on
        // it. Without this the final action sets state that nothing reads and the
        // console simply stays where it was.
        if let Some(onboarding) = shell.onboarding.clone() {
            let weak = cx.entity().downgrade();
            shell._onboarding_result = Some(cx.observe(&onboarding, move |_, view, app| {
                let Some(result) = view.read(app).result().cloned() else {
                    return;
                };
                let weak = weak.clone();
                // Deferred, not immediate: the observer runs while the shell is already
                // being updated (the press arrived through it), and a nested update of
                // the same entity panics. One turn later is also when the result is
                // genuinely final.
                app.spawn(async move |_, app| {
                    weak.update(app, |shell: &mut AppShell, cx| {
                        if shell.onboarding_completed {
                            return;
                        }
                        shell.onboarding_completed = true;
                        shell.complete_onboarding(result, cx);
                    })
                    .ok();
                })
                .detach();
            }));
        }

        cx.spawn_in(window, async move |this, cx| {
            this.update_in(cx, |shell, window, cx| {
                // Menu commands route from the focused node, and the shell's root is
                // where the Appearance and dock handlers live. Until it holds focus,
                // those commands are dispatched into nothing: the handlers exist and
                // are never reached.
                shell.focus.focus(window, cx);
                shell.run_bootstrap(window, cx);
            })
            .ok();
        })
        .detach();
        shell
    }

    /// The bootstrap sequence, run as a task so the splash can actually paint.
    fn run_bootstrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.paths.clone();
        let paths_error = self.paths_error.clone();
        let config = self.config.clone();
        let store_root = self.store.root().to_path_buf();

        cx.spawn_in(window, async move |this, cx| {
            let work = async move { bootstrap(paths, paths_error, config, store_root) };
            let outcome = cx.background_spawn(work).await;

            this.update_in(cx, |shell, _window, cx| {
                shell.apply_bootstrap(outcome, cx);
            })
            .ok();
        })
        .detach();
    }

    fn apply_bootstrap(
        &mut self,
        outcome: Result<BootstrapOutcome, BootstrapFailure>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(outcome) => {
                if outcome.steps.first().is_some_and(BootStep::is_failed) {
                    self.steps[0] = outcome.steps[0].clone();
                    return;
                }
                self.config = outcome.config;
                self.profile = outcome.profile;
                self.store = outcome.store;
                self.session = outcome.session;
                self.steps = outcome.steps;
                self.route = if self.config.is_bootstrapped() {
                    Route::Producer
                } else {
                    Route::Onboarding
                };
                self.ensure_screen(cx);
            }
            Err(failure) => {
                if let Some(step) = self.steps.first_mut() {
                    *step = BootStep::failed(step.label.clone(), failure.to_string());
                }
            }
        }
        cx.notify();
    }

    /// The window root's focus handle, so a dispatched action can reach it.
    #[must_use]
    pub fn focus_handle(&self) -> &gpui_kit::FocusHandle {
        &self.focus
    }

    /// The onboarding screen, once it exists.
    #[must_use]
    pub fn onboarding(&self) -> Option<&Entity<OnboardingView>> {
        self.onboarding.as_ref()
    }

    /// Whether a File command is waiting on a path.
    ///
    /// True from choosing File → Import or File → Export until the path is confirmed
    /// or the dialog is dismissed. The dialog itself is drawn by the window's overlay,
    /// so this is how the shell's own half of the exchange is observed.
    #[must_use]
    pub fn file_dialog_open(&self) -> bool {
        self.file_dialog.is_some() && self.path_input.is_some()
    }

    /// The producer screen, once one exists.
    #[must_use]
    pub fn producer(&self) -> Option<Entity<ProducerView>> {
        self.producer.clone()
    }

    /// Which screen is showing.
    #[must_use]
    pub const fn route(&self) -> Route {
        self.route
    }

    /// Creates the producer screen for the current route if it does not exist yet.
    ///
    /// Screens are retained so their state survives a trip through the other one.
    fn ensure_screen(&mut self, cx: &mut Context<Self>) {
        if self.route == Route::Producer && self.producer.is_none() {
            let context = ProducerContext {
                party: self.party_summary.clone(),
                config: self.config.clone(),
                profile: self.profile.clone(),
                engine: self.engine.clone(),
                identity: self.session.auth().identity().cloned().unwrap_or_else(|| {
                    partytime_api::Identity {
                        user_id: String::new(),
                        handle: self.config.account.clone().unwrap_or_default(),
                        display_name: String::new(),
                    }
                }),
            };
            self.producer = Some(cx.new(|cx| ProducerView::new(context, cx)));
        }
    }

    /// Moves to a route, creating its screen if needed.
    pub fn go_to(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route == route {
            return;
        }
        self.route = route;
        self.ensure_screen(cx);
        cx.notify();
    }

    /// Finishes onboarding: remembers the choices and opens the producer view.
    pub fn complete_onboarding(
        &mut self,
        result: crate::onboarding::OnboardingResult,
        cx: &mut Context<Self>,
    ) {
        let crate::onboarding::OnboardingResult {
            profile,
            party,
            account,
            party_summary,
        } = result;
        self.party_summary = party_summary;
        self.config.profile = Some(profile.clone());
        // The producer view renders a profile; load the one just chosen rather than
        // whatever happened to be in memory at launch.
        self.profile = self.store.load(&profile).ok();
        self.config.party = Some(party);
        self.config.account = Some(account);
        if let Some(paths) = &self.paths {
            // A failure to remember the choices is not fatal: the console still opens,
            // it just asks again next launch.
            self.config.save(&paths.settings_file()).ok();
        }
        self.producer = None;
        self.go_to(Route::Producer, cx);
    }

    /// The failure the splash should show, if the bootstrap failed.
    pub fn boot_failure(&self) -> Option<SharedString> {
        first_failure(&self.steps)
    }

    /// Whether the bootstrap has settled.
    pub fn boot_finished(&self) -> bool {
        bootstrap_complete(&self.steps)
    }
}

impl AppShell {
    /// View → Appearance → System: follow the desktop again.
    fn appearance_system(&mut self, _: &AppearanceSystem, _w: &mut Window, cx: &mut Context<Self>) {
        theme::set_preference(theme::ThemePreference::System, cx);
        cx.notify();
    }

    /// View → Appearance → Light.
    fn appearance_light(&mut self, _: &AppearanceLight, _w: &mut Window, cx: &mut Context<Self>) {
        theme::set_preference(theme::ThemePreference::Light, cx);
        cx.notify();
    }

    /// Profile → Switch profile: return to the profile step.
    fn switch_profile(&mut self, _: &SwitchProfile, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.onboarding.clone() {
            view.update(cx, |view, cx| view.clear_profile(cx));
        }
        // Re-arm the completion observer, which is one-shot by design.
        self.onboarding_completed = false;
        self.producer = None;
        self.go_to(Route::Onboarding, cx);
    }

    /// Help → About PartyTime.
    ///
    /// Drawn inline by the shell rather than in the window's dialog overlay, for the
    /// same reason as the path prompt: the overlay is outside the window's own render,
    /// so nothing in it can be hit-tested or observed.
    fn about(&mut self, _: &AboutPartyTime, _window: &mut Window, cx: &mut Context<Self>) {
        self.about_open = true;
        cx.notify();
    }

    /// File → Import OBS profile…
    fn import_obs_profile(
        &mut self,
        _: &ImportObsProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_dialog(FileDialog::Import, window, cx);
    }

    /// File → Export profile…
    fn export_profile(&mut self, _: &ExportProfile, window: &mut Window, cx: &mut Context<Self>) {
        self.open_file_dialog(FileDialog::Export, window, cx);
    }

    /// File → Quit.
    fn quit_app(&mut self, _: &Quit, _window: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    /// Asks for a path. Both File commands share one prompt; `FileDialog` is what tells
    /// Confirm whether it is reading a collection or writing one.
    fn open_file_dialog(&mut self, which: FileDialog, window: &mut Window, cx: &mut Context<Self>) {
        self.file_error = None;
        self.file_dialog = Some(which);
        let field = cx.new(|cx| InputState::new(window, cx));
        self.path_input = Some(field.clone());

        // Enter confirms as well as the button, because a path is exactly the kind of
        // value people type and then press Enter.
        self.file_watch = Some(cx.subscribe(&field, |shell, field, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let path = field.read(cx).value().to_string();
                shell.apply_file_dialog(cx, path);
            }
        }));
        cx.notify();
    }

    /// Runs whichever File command the prompt was opened for.
    fn apply_file_dialog(&mut self, cx: &mut Context<Self>, path: String) {
        let Some(which) = self.file_dialog.take() else {
            return;
        };
        self.path_input = None;
        let path = PathBuf::from(path.trim());
        match which {
            FileDialog::Import => match self.store.import_from_obs(&path) {
                Ok(profile) => {
                    self.profile = Some(profile);
                    // The producer holds a snapshot of the profile, so it is rebuilt.
                    self.producer = None;
                    self.ensure_screen(cx);
                    self.file_error = None;
                }
                Err(err) => self.file_error = Some(err.to_string().into()),
            },
            FileDialog::Export => match self.profile.clone() {
                Some(profile) => match self.store.export_to_obs(&profile, &path) {
                    Ok(()) => self.file_error = None,
                    Err(err) => self.file_error = Some(err.to_string().into()),
                },
                None => self.file_error = Some("There is no profile to export yet.".into()),
            },
        }
        cx.notify();
    }

    /// Edit → Undo: hands the command to the producer, which owns the history.
    fn undo_edit(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.producer.clone() {
            view.update(cx, |view, cx| view.undo(cx));
        }
    }

    /// Edit → Redo.
    fn redo_edit(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.producer.clone() {
            view.update(cx, |view, cx| view.redo(cx));
        }
    }

    /// Docks → Left.
    ///
    /// The menu bar belongs to the shell, so a dock command chosen there is dispatched
    /// from the shell's focused node. Without this hop the producer's own handler never
    /// sees it and the menu entry quietly does nothing.
    fn toggle_left_dock(
        &mut self,
        _: &ToggleLeftDock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.producer.clone() {
            view.update(cx, |view, cx| view.toggle_left_dock(cx));
        }
    }

    /// Docks → Right.
    fn toggle_right_dock(
        &mut self,
        _: &ToggleRightDock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.producer.clone() {
            view.update(cx, |view, cx| view.toggle_right_dock(cx));
        }
    }

    /// View → Appearance → Dark.
    fn appearance_dark(&mut self, _: &AppearanceDark, _w: &mut Window, cx: &mut Context<Self>) {
        theme::set_preference(theme::ThemePreference::Dark, cx);
        cx.notify();
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let about_panel = self.about_open.then(|| {
            modal(
                v_flex()
                    .id("about-panel")
                    .test_support()
                    .border_1()
                    .border_color(cx.theme().border)
                    .p_4()
                    .gap_2()
                    .child(div().id("about-title").child("PartyTime"))
                    .child(
                        div()
                            .id("about-version")
                            .test_support()
                            .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .id("about-what")
                            .child("Publishing console for OpenParty.tv"),
                    )
                    .child(
                        Button::new("about-close")
                            .label("Close")
                            .on_click(cx.listener(|shell: &mut Self, _, _, cx| {
                                shell.about_open = false;
                                cx.notify();
                            })),
                    ),
            )
        });
        // The path prompt is drawn by the shell rather than in the window's dialog
        // overlay: the overlay is not part of the window's own render, so a control
        // placed there is outside the UI's hit testing and cannot be observed at all.
        let file_panel =
            self.file_dialog.map(|which| {
                let field = self.path_input.clone();
                v_flex()
                    .id("file-panel")
                    .child(div().id("file-title").child(which.title()))
                    .child(div().id("file-blurb").child(which.blurb()))
                    .children(field.map(|state| {
                        div()
                            .id("file-path")
                            .test_support()
                            .child(Input::new(&state))
                    }))
                    .child(
                        h_flex()
                            .id("file-actions")
                            .child(Button::new("file-cancel").label("Cancel").on_click(
                                cx.listener(|shell: &mut Self, _, _, cx| {
                                    shell.file_dialog = None;
                                    shell.path_input = None;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("file-confirm")
                                    .label(which.confirm_label())
                                    .on_click(cx.listener(|shell: &mut Self, _, _, cx| {
                                        let path = shell
                                            .path_input
                                            .as_ref()
                                            .map(|state| state.read(cx).value().to_string())
                                            .unwrap_or_default();
                                        shell.apply_file_dialog(cx, path);
                                    })),
                            ),
                    )
                    .into_any_element()
            });
        // The same test `TitleBar` uses, so exactly one layer draws the buttons. Drawing
        // both is what put six icons on screen on a client-decorated session.
        let controls = (!matches!(
            window.window_decorations(),
            gpui_kit::gpui::Decorations::Client { .. }
        ))
        .then(|| {
            gpui_kit::base::h_flex()
                .items_center()
                .gap_1()
                .child(
                    button(
                        "win-minimize",
                        gpui_kit::component::IconName::WindowMinimize,
                    )
                    .on_click(|_, window, _| window.minimize_window()),
                )
                .child(
                    button(
                        "win-maximize",
                        if window.is_maximized() {
                            gpui_kit::component::IconName::WindowRestore
                        } else {
                            gpui_kit::component::IconName::WindowMaximize
                        },
                    )
                    .on_click(|_, window, _| window.zoom_window()),
                )
                .child(
                    button("win-close", gpui_kit::component::IconName::WindowClose)
                        .on_click(|_, window, _| window.remove_window()),
                )
                .into_any_element()
        });
        let route = self.route;
        let splash = || Splash {
            steps: self.steps.clone(),
        };
        let screen: AnyElement = match route {
            Route::Splash => splash().into_any_element(),
            Route::Onboarding => match &self.onboarding {
                Some(view) => view.clone().into_any_element(),
                None => splash().into_any_element(),
            },
            Route::Producer => match &self.producer {
                Some(view) => view.clone().into_any_element(),
                None => splash().into_any_element(),
            },
        };

        v_flex()
            .id("shell-root")
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            // Client-side chrome, drawn by us rather than the window manager.
            //
            // GNOME has no platform menu bar, so the menu is our own element. It also
            // does not grant `Decorations::Client` on X11, which is the flag gpui-kit's
            // `TitleBar` gates its window buttons on — so those buttons never appear
            // either. Drawing the controls directly is the only version that works here.
            .child(
                gpui_kit::component::TitleBar::new()
                    .child(crate::menu::menu_bar())
                    .child(
                        gpui_kit::base::h_flex()
                            .flex_1()
                            .justify_end()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .pr_2()
                                    .child("PartyTime"),
                            )
                            // Empty when `TitleBar` is drawing its own.
                            .children(controls),
                    ),
            )
            .on_action(cx.listener(Self::appearance_system))
            .on_action(cx.listener(Self::appearance_light))
            .on_action(cx.listener(Self::appearance_dark))
            .on_action(cx.listener(Self::about))
            .on_action(cx.listener(Self::switch_profile))
            .on_action(cx.listener(Self::import_obs_profile))
            .on_action(cx.listener(Self::export_profile))
            .on_action(cx.listener(Self::quit_app))
            .on_action(cx.listener(Self::undo_edit))
            .on_action(cx.listener(Self::redo_edit))
            .on_action(cx.listener(Self::toggle_left_dock))
            .on_action(cx.listener(Self::toggle_right_dock))
            .children(file_panel)
            .children(about_panel)
            // A File command that could not do its job says so here. Silently swallowing
            // it is the one thing this console must not do: a creator who thinks they
            // imported a collection has to learn that they did not.
            .children(self.file_error.clone().map(|error| {
                div()
                    .id("file-error")
                    .test_support()
                    .child(error)
                    .into_any_element()
            }))
            // The only motion in the shell: a short fade that makes the handover from
            // one screen to the next legible. Keyed by route so it replays on each
            // change, and automatically skipped when the system asks for reduced motion.
            .child(
                div()
                    .size_full()
                    .with_animation(
                        ("route", route.index()),
                        Animation::new(Duration::from_millis(180)),
                        |this, value| this.opacity(value),
                    )
                    .child(screen),
            )
    }
}

/// What the bootstrap produced.
#[derive(Debug)]
pub struct BootstrapOutcome {
    /// Remembered settings.
    pub config: ConsoleConfig,
    /// The profile to load, if one was remembered and present.
    pub profile: Option<Profile>,
    /// The profile store.
    pub store: ProfileStore,
    /// The session.
    pub session: ConsoleSession,
    /// The steps that ran.
    pub steps: Vec<BootStep>,
}

/// Why the bootstrap could not run at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BootstrapFailure {
    /// The configuration directory could not be used.
    #[error("{0}")]
    Paths(String),
}

/// The filesystem work the splash is reporting on.
///
/// Split out so it can be tested without a window.
pub fn bootstrap(
    paths: Option<Paths>,
    paths_error: Option<SharedString>,
    config: ConsoleConfig,
    store_root: std::path::PathBuf,
) -> Result<BootstrapOutcome, BootstrapFailure> {
    let Some(paths) = paths else {
        return Err(BootstrapFailure::Paths(paths_error.map_or_else(
            || PathsError::NoHome.to_string(),
            |why| why.to_string(),
        )));
    };
    let session = ConsoleSession::new(config.origin.clone());
    let mut steps = vec![BootStep::running("Read configuration")];
    steps.push(BootStep::pending("Load profile"));
    steps.push(BootStep::pending("Start media engine"));

    if let Err(err) = paths.ensure() {
        steps[0] = BootStep::failed("Read configuration", err.to_string());
        return Ok(BootstrapOutcome {
            config,
            profile: None,
            store: ProfileStore::new(&store_root),
            session,
            steps,
        });
    }
    steps[0] = BootStep::done("Read configuration");

    let store = ProfileStore::new(&paths.profiles_dir);
    let profile = match config.profile.as_deref() {
        Some(name) => match store.load(name) {
            Ok(profile) => {
                steps[1] = BootStep::done("Load profile");
                Some(profile)
            }
            Err(err) => {
                steps[1] = BootStep::failed("Load profile", err.to_string());
                None
            }
        },
        None => {
            steps[1] = BootStep::done("Load profile");
            None
        }
    };

    // The S2 core smoke is real, but the console does not yet keep a long-lived runtime
    // or connect its profile to it. Keep that blocker visible instead of marking startup
    // complete as if publishing were ready.
    steps[2] = BootStep::failed(
        "Start media engine",
        "The libobs runtime is not yet integrated into the console.",
    );

    Ok(BootstrapOutcome {
        session,
        config,
        profile,
        store,
        steps,
    })
}

/// One window-control button: quiet until hovered, square, no label.
fn button(
    id: &'static str,
    icon: gpui_kit::component::IconName,
) -> gpui_kit::component::button::Button {
    use gpui_kit::component::{Sizable as _, button::ButtonVariants as _};
    Button::new(id).ghost().small().icon(icon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splash::StepState;

    fn temp_paths(tag: &str) -> Paths {
        let dir = std::env::temp_dir().join(format!("pt-boot-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Paths::under(dir)
    }

    #[test]
    fn bootstrap_creates_directories_and_names_unintegrated_engine() {
        let paths = temp_paths("ok");
        let outcome = bootstrap(
            Some(paths.clone()),
            None,
            ConsoleConfig::default(),
            paths.profiles_dir.clone(),
        )
        .expect("bootstrap runs");
        assert!(bootstrap_complete(&outcome.steps), "{:?}", outcome.steps);
        assert!(
            matches!(&outcome.steps[2].state, StepState::Failed(reason)
                if reason == "The libobs runtime is not yet integrated into the console."),
            "the splash must not call an absent runtime ready: {:?}",
            outcome.steps[2]
        );
        assert!(paths.config_dir.is_dir());
        assert!(paths.profiles_dir.is_dir());
        let _ = std::fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn bootstrap_loads_the_remembered_profile_when_it_exists() {
        let paths = temp_paths("profile");
        let mut store = ProfileStore::new(&paths.profiles_dir);
        store.save(&Profile::starter("Friday Night")).expect("save");

        let config = ConsoleConfig {
            profile: Some("Friday Night".into()),
            party: Some("party:abc".into()),
            ..ConsoleConfig::default()
        };
        let outcome = bootstrap(
            Some(paths.clone()),
            None,
            config,
            paths.profiles_dir.clone(),
        )
        .expect("bootstrap runs");
        let profile = outcome.profile.expect("profile loaded");
        assert_eq!(profile.name, "Friday Night");
        assert!(bootstrap_complete(&outcome.steps));
        let _ = std::fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn a_remembered_profile_that_is_gone_fails_the_step_rather_than_the_launch() {
        let paths = temp_paths("missing");
        let config = ConsoleConfig {
            profile: Some("Deleted".into()),
            party: Some("party:abc".into()),
            ..ConsoleConfig::default()
        };
        let outcome = bootstrap(
            Some(paths.clone()),
            None,
            config,
            paths.profiles_dir.clone(),
        )
        .expect("runs");
        assert!(outcome.profile.is_none());
        assert!(
            first_failure(&outcome.steps).is_some(),
            "the step must report why"
        );
        let _ = std::fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn bootstrap_reports_an_unusable_configuration_directory() {
        // A path under a regular file cannot be created.
        let blocker = std::env::temp_dir().join(format!("pt-file-{}", std::process::id()));
        std::fs::write(&blocker, b"x").expect("write");
        let paths = Paths::under(blocker.join("nested"));
        let outcome = bootstrap(
            Some(paths),
            None,
            ConsoleConfig::default(),
            std::path::PathBuf::from("/tmp"),
        )
        .expect("runs");
        assert!(first_failure(&outcome.steps).is_some());
        let _ = std::fs::remove_file(&blocker);
    }

    #[test]
    fn an_unresolvable_configuration_directory_is_a_bootstrap_failure() {
        let err = bootstrap(
            None,
            Some("no home directory is set and PARTYTIME_CONFIG_DIR is not set".into()),
            ConsoleConfig::default(),
            std::path::PathBuf::from("/tmp"),
        )
        .expect_err("must fail");
        assert!(matches!(err, BootstrapFailure::Paths(_)));
    }

    #[test]
    fn routes_have_distinct_identities_so_the_fade_replays() {
        assert_ne!(Route::Splash.index(), Route::Onboarding.index());
        assert_ne!(Route::Onboarding.index(), Route::Producer.index());
    }
}
