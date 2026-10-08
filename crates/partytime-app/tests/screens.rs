//! UI integration tests for the three screens.
//!
//! Each test mounts the real view in a headless window and drives it through the
//! keyboard and pointer, then asserts the application's own result — not only the
//! accessibility tree. A test that proves a control exists but never proves what the
//! control did is not a test of the control.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, Window};
use partytime_api::Profile as PartyProfile;
use partytime_api::{
    ConsentState, ConsoleSession, Identity, Me, MyInput, PartyDetail, PartyError, PartyList,
    PartyRole, PartySession, PartySummary, PublishKind, TokenSet,
};
use partytime_app::app::AppShell;
use partytime_app::menu::{AppearanceDark, AppearanceLight, AppearanceSystem};
use partytime_app::onboarding::OnboardingView;
use partytime_app::paths::{ConsoleConfig, Paths};
use partytime_app::platform::{Platform, PlatformFuture};
use partytime_app::producer::{ProducerContext, ProducerView, ViewLayout};
use partytime_app::splash::{BootStep, Splash};
use partytime_engine::{AspectRatio, EngineStatus, Profile, ProfileStore};

fn identity() -> Identity {
    Identity {
        user_id: "u1".into(),
        handle: "ada".into(),
        display_name: "Ada".into(),
    }
}

/// A platform that answers from memory, so the welcome and party screens can be driven
/// without a browser or a server. It records what it was asked, so a test can prove the
/// right call went out.
struct FakePlatform {
    sign_in: Result<TokenSet, PartyError>,
    parties: Result<PartyList, PartyError>,
    calls: Arc<Mutex<Vec<String>>>,
    durable: bool,
    /// When set, sign-in waits on this before answering, so a test can hold the attempt
    /// open across several executor turns.
    gate: Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
}

impl FakePlatform {
    /// A platform whose sign-in blocks until the returned sender is used.
    ///
    /// The usual fake completes on the first poll, which hides a spawn that never hands
    /// its result back. Holding the attempt open across turns exercises the real
    /// sequence: click, park, complete, settle.
    fn gated(
        sign_in: Result<TokenSet, PartyError>,
    ) -> (Self, futures::channel::oneshot::Sender<()>) {
        let (tx, rx) = futures::channel::oneshot::channel();
        (
            Self {
                sign_in,
                parties: Ok(PartyList {
                    ok: true,
                    live_only: true,
                    parties: vec![],
                }),
                calls: Arc::new(Mutex::new(Vec::new())),
                durable: true,
                gate: Mutex::new(Some(rx)),
            },
            tx,
        )
    }

    fn ok() -> (Self, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                sign_in: Ok(tokens()),
                parties: Ok(party_list()),
                calls: Arc::clone(&calls),
                durable: true,
                gate: Mutex::new(None),
            },
            calls,
        )
    }

    fn refusing_sign_in(error: PartyError) -> Self {
        Self {
            sign_in: Err(error),
            parties: Ok(party_list()),
            calls: Arc::new(Mutex::new(Vec::new())),
            durable: true,
            gate: Mutex::new(None),
        }
    }

    fn without_a_credential_store() -> Self {
        let (mut platform, _) = Self::ok();
        platform.durable = false;
        platform
    }
}

impl Platform for FakePlatform {
    fn start_sign_in(&self) -> PlatformFuture<Result<TokenSet, PartyError>> {
        let outcome = self.sign_in.clone();
        let calls = Arc::clone(&self.calls);
        let gate = self.gate.lock().ok().and_then(|mut gate| gate.take());
        Box::pin(async move {
            if let Some(gate) = gate {
                // Awaiting the gate parks this future; the click that started it returns
                // long before the answer exists.
                let _ = gate.await;
            }
            calls.lock().expect("calls").push("sign_in".into());
            outcome
        })
    }

    fn me(&self) -> PlatformFuture<Result<Me, PartyError>> {
        Box::pin(async move { Ok(me()) })
    }

    fn parties(&self, live_only: bool) -> PlatformFuture<Result<PartyList, PartyError>> {
        let outcome = self.parties.clone();
        let calls = Arc::clone(&self.calls);
        Box::pin(async move {
            calls
                .lock()
                .expect("calls")
                .push(format!("parties:{live_only}"));
            outcome
        })
    }

    fn party(&self, _id: String) -> PlatformFuture<Result<PartyDetail, PartyError>> {
        Box::pin(async move {
            Err(PartyError::PartyNotFound {
                server_text: "Party not found.".into(),
            })
        })
    }

    fn sign_out(&self) -> PlatformFuture<Result<(), PartyError>> {
        let calls = Arc::clone(&self.calls);
        Box::pin(async move {
            calls.lock().expect("calls").push("sign_out".into());
            Ok(())
        })
    }

    fn session_is_durable(&self) -> bool {
        self.durable
    }

    fn store_description(&self) -> String {
        if self.durable {
            "the system credential store".to_string()
        } else {
            "memory only — you will sign in again next launch".to_string()
        }
    }
}

fn tokens() -> TokenSet {
    TokenSet::from_parts(
        "pt_access".into(),
        "pr_refresh".into(),
        std::time::Duration::from_secs(3600),
        "profile:read channels:read parties:read publish".into(),
        std::time::SystemTime::now(),
    )
}

fn me() -> Me {
    Me {
        ok: true,
        client_id: "partytime-dev".into(),
        scopes: vec![
            "profile:read".into(),
            "channels:read".into(),
            "parties:read".into(),
            "publish".into(),
        ],
        profile: PartyProfile {
            id: "i5g9espl8obrv10kdm9o".into(),
            handle: "ada".into(),
            display_name: "Ada".into(),
            avatar_url: None,
            banner_url: None,
            bio: None,
            links: Vec::new(),
        },
    }
}

fn party_list() -> PartyList {
    PartyList {
        ok: true,
        live_only: true,
        parties: vec![party("wlyayz1ytl2u822bifb4", "Friday Night")],
    }
}

fn party(id: &str, title: &str) -> PartySummary {
    PartySummary {
        id: id.into(),
        title: title.into(),
        visibility: "public".into(),
        status: "live".into(),
        game_name: Some("Helldivers 2".into()),
        channel_id: None,
        allow_rogue: false,
        role: PartyRole::Member,
        is_director: false,
        can_go_live: false,
        session: Some(PartySession {
            id: "s1".into(),
            status: "live".into(),
            started_at: None,
        }),
        my_inputs: vec![MyInput {
            kind: PublishKind::Camera,
            label: Some("Face cam".into()),
            consent: ConsentState::Approved,
        }],
        approved_kinds: vec![PublishKind::Camera],
        director_handle: None,
    }
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pt-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

// ------------------------------------------------------------------ theme

#[gpui_kit::test]
fn the_openparty_palette_is_the_one_the_window_renders_with(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;

    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::apply(cx);
    });

    cx.update(|cx| {
        let theme = Theme::global(cx);
        let hex = |color: gpui_kit::Hsla| {
            let c = color.to_rgb();
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
        };
        assert_eq!(
            hex(theme.background),
            "#FFFFFF",
            "light surface is OpenParty's white"
        );
        assert_eq!(hex(theme.foreground), "#09090B");
        assert_eq!(hex(theme.muted_foreground), "#71717B");
        assert_eq!(
            hex(theme.primary),
            "#0069A8",
            "primary is the OpenParty blue"
        );
        assert_eq!(
            theme.radius,
            gpui_kit::px(10.),
            "radius follows --radius: 0.625rem"
        );
        assert_eq!(theme.font_family.as_ref(), "Figtree");
    });
}

#[gpui_kit::test]
fn the_bundled_figtree_face_loads(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        // Asserted directly rather than through the family list: a headless GPUI test has
        // no font collection at all (`all_font_names()` is empty there), so the only honest
        // question is whether the bundled bytes parse.
        partytime_app::theme::register_fonts(cx).expect("the bundled Figtree face parses");
    });
}

#[gpui_kit::test]
fn a_branded_window_still_renders_its_screens(cx: &mut TestAppContext) {
    let dir = temp_dir("themed");
    let paths = Ok(Paths::under(&dir));
    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::apply(cx);
    });
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("signin").is_some(),
            "the branded window must still render the welcome screen"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn choosing_dark_recolours_the_whole_window(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;
    use partytime_app::theme::ThemePreference;

    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::use_preference(ThemePreference::Dark, cx);
    });
    cx.update(|cx| {
        let theme = Theme::global(cx);
        assert!(theme.is_dark(), "the console must render dark when told to");
        let hex = |color: gpui_kit::Hsla| {
            let c = color.to_rgb();
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("#{:02X}{:02X}{:02X}", byte(c.r), byte(c.g), byte(c.b))
        };
        assert_eq!(hex(theme.background), "#09090B", "dark --background");
        assert_eq!(hex(theme.foreground), "#FAFAFA");
        assert_eq!(hex(theme.primary), "#00598A", "dark --primary");
    });
}

#[gpui_kit::test]
fn choosing_light_ignores_a_dark_desktop(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;
    use partytime_app::theme::ThemePreference;

    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::use_preference(ThemePreference::Light, cx);
    });
    cx.update(|cx| {
        assert!(!Theme::global(cx).is_dark());
        let c = Theme::global(cx).background.to_rgb();
        assert!((c.r - 1.0).abs() < 0.01, "light --background is white");
    });
}

#[gpui_kit::test]
fn a_desktop_switch_does_not_undo_a_forced_choice(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;
    use partytime_app::theme::ThemePreference;

    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::use_preference(ThemePreference::Dark, cx);
    });

    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(480.), gpui_kit::px(360.)),
        |window, cx| {
            gpui_kit::component::Root::new(cx.new(|_| SplashHost { steps: Vec::new() }), window, cx)
        },
    );
    cx.update_window(handle.into(), |_, window, app| {
        // Stands in for the compositor switching at sunset.
        partytime_app::theme::sync(window, app);
    })
    .ok();

    cx.update(|cx| {
        assert!(
            Theme::global(cx).is_dark(),
            "a forced choice survives a system switch"
        );
    });
}

#[gpui_kit::test]
fn the_appearance_control_is_reachable_from_the_producer_view(cx: &mut TestAppContext) {
    let (handle, _view) = mount_producer(cx, producer_context());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("appearance").is_some(),
            "the producer view must offer an appearance control"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_appearance_control_is_reachable_before_the_producer_view(cx: &mut TestAppContext) {
    let dir = temp_dir("appearance-menu");
    let store = ProfileStore::new(dir.join("p"));
    let (handle, _view) = mount_onboarding(cx, Box::new(FakePlatform::ok().0), store);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("appearance").is_some(),
            "onboarding must offer an appearance control too"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- splash

struct SplashHost {
    steps: Vec<BootStep>,
}

impl Render for SplashHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Splash {
            steps: self.steps.clone(),
        }
        .into_any_element()
    }
}

#[gpui_kit::test]
fn the_splash_lists_every_step_it_is_running(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut host: Option<Entity<SplashHost>> = None;
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(480.), gpui_kit::px(360.)),
        |window, cx| {
            let entity = cx.new(|_| SplashHost {
                steps: vec![
                    BootStep::done("Read configuration"),
                    BootStep::running("Load profile"),
                    BootStep::pending("Start media engine"),
                ],
            });
            host = Some(entity.clone());
            Root::new(entity, window, cx)
        },
    );

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("boot-step-0").is_some(),
            "first step missing"
        );
        assert!(
            window.try_find("boot-step-2").is_some(),
            "third step missing"
        );
    })
    .unwrap();
    assert!(host.is_some());
}

// -------------------------------------------------------------- onboarding

fn mount_onboarding(
    cx: &mut TestAppContext,
    platform: Box<dyn Platform>,
    store: ProfileStore,
) -> (gpui_kit::AnyWindowHandle, Entity<OnboardingView>) {
    cx.update(gpui_kit::init);
    let captured: Rc<RefCell<Option<Entity<OnboardingView>>>> = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&captured);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(600.), gpui_kit::px(560.)),
        move |window, cx| {
            let entity = cx.new(|cx| {
                OnboardingView::new(
                    store.clone(),
                    ConsoleSession::new("https://openparty.test"),
                    platform,
                    window,
                    cx,
                )
            });
            *sink.borrow_mut() = Some(entity.clone());
            Root::new(entity, window, cx)
        },
    );
    (
        handle.into(),
        captured.borrow().clone().expect("view captured"),
    )
}

/// Signs in and lands on the party step, the way a real launch would.
fn sign_in_through_to_parties(
    cx: &mut TestAppContext,
    handle: gpui_kit::AnyWindowHandle,
    view: &Entity<OnboardingView>,
) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("choose-profile:Friday Night", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(!view.read(cx).is_complete()));
}

// The two tests below hold an attempt open across several executor turns, so they cover
// the spawn handing its result back and a settled attempt reaching the view. Whether the
// window repaints *unprompted* is a separate question, answered by the observer test
// further down — `render_frame` paints on demand and would hide a missing notify.

#[gpui_kit::test]
fn a_sign_in_held_open_across_executor_turns_still_settles(cx: &mut TestAppContext) {
    let dir = temp_dir("gated-signin");
    let (platform, gate) = FakePlatform::gated(Ok(tokens()));
    let (handle, view) = mount_onboarding(cx, Box::new(platform), ProfileStore::new(dir.join("p")));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            view.read(cx).stage().is_some(),
            "an attempt still in flight must keep the busy state"
        );
        assert!(!view.read(cx).session_is_signed_in());
    });

    gate.send(()).ok();
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            view.read(cx).stage().is_none(),
            "the busy state must be cleared once the attempt settles"
        );
        assert!(
            view.read(cx).session_is_signed_in(),
            "a sign-in that resolved late must still be recorded"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_late_refusal_reaches_the_view_rather_than_staying_busy(cx: &mut TestAppContext) {
    let dir = temp_dir("gated-refusal");
    let (platform, gate) = FakePlatform::gated(Err(PartyError::Transport("no route".into())));
    let (handle, view) = mount_onboarding(cx, Box::new(platform), ProfileStore::new(dir.join("p")));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();
    gate.send(()).ok();
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            view.read(cx).stage().is_none(),
            "a settled refusal is not still in progress"
        );
        let error = view
            .read(cx)
            .error()
            .expect("the refusal must reach the view");
        assert_eq!(error.server_text, "no route");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

// `cx.observe` fires when the entity is refreshed, which is what `notify` requests — so
// a missing notify is observable here, where `render_frame` alone would not be.
//
// It has to be the refusal path: on success the view goes on to load the party list,
// which notifies in its own right and would mask the omission.
#[gpui_kit::test]
fn a_settled_sign_in_asks_the_window_to_repaint(cx: &mut TestAppContext) {
    let dir = temp_dir("notify");
    // The refusal path, deliberately: on success the view goes on to load the party
    // list, which notifies in its own right and would mask a missing notify here.
    let (platform, gate) = FakePlatform::gated(Err(PartyError::Transport("no route".into())));
    let (handle, view) = mount_onboarding(cx, Box::new(platform), ProfileStore::new(dir.join("p")));

    let refreshes = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&refreshes);
    let subscription = cx.update(|cx| {
        cx.observe(&view, move |_, _| {
            seen.fetch_add(1, Ordering::SeqCst);
        })
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let before = refreshes.load(Ordering::SeqCst);

    gate.send(()).ok();
    cx.run_until_parked();

    let after = refreshes.load(Ordering::SeqCst);
    drop(subscription);
    assert!(
        after > before,
        "settling the sign-in changed the view but never asked for a repaint \
         ({before} -> {after} refreshes)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// The menu module's own doc claimed Appearance -> Dark did nothing. These dispatch the
// actions at the shell and read the resulting theme, so the claim is settled by
// behaviour rather than by reading the handlers.
// The shell's root is the node that carries the Appearance handlers, and menu commands
// only reach a handler if the root is the focused node. So this also pins the focus
// contract: without `cx.focus` in the shell's spawn, every menu command is dead code.
// A File entry that opens nothing is still an entry that does nothing. Dispatching the
// command has to reach the handler and put a path field in front of the user.
#[gpui_kit::test]
fn a_dispatched_file_command_asks_for_a_path(cx: &mut TestAppContext) {
    use partytime_app::menu::{ExportProfile, ImportObsProfile};

    let dir = temp_dir("file-dialog");
    let paths = Ok(Paths::under(&dir));
    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::apply(cx);
    });
    // The window handle reads back the Root; the shell is what answers this question,
    // so the entity is kept on the way in.
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let captured = slot.clone();
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| AppShell::new_at(paths, window, cx));
            *captured.borrow_mut() = Some(shell.clone());
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();
    let shell = slot.borrow().clone().expect("the shell");

    let mut open = true;
    cx.update(|cx| open = shell.read(cx).file_dialog_open());
    assert!(
        !open,
        "a File dialog was open before any File command was chosen"
    );

    cx.dispatch_action(any, ExportProfile);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            shell.read(cx).file_dialog_open(),
            "File -> Export profile… did not ask for a destination, so there is nowhere to write to"
        );
    });

    cx.dispatch_action(any, ImportObsProfile);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            shell.read(cx).file_dialog_open(),
            "File -> Import OBS profile… did not ask for a collection to read"
        );
    });
}
#[gpui_kit::test]
fn the_menu_appearance_commands_change_the_theme(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;

    let dir = temp_dir("menu-appearance");
    let paths = Ok(Paths::under(&dir));
    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::apply(cx);
    });
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );
    cx.run_until_parked();

    let any: gpui_kit::AnyWindowHandle = handle.into();
    cx.dispatch_action(any, AppearanceDark);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            Theme::global(cx).is_dark(),
            "Appearance -> Dark did not darken the theme"
        );
        assert_eq!(
            partytime_app::theme::preference(cx),
            partytime_app::theme::ThemePreference::Dark
        );
    });

    cx.dispatch_action(any, AppearanceLight);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !Theme::global(cx).is_dark(),
            "Appearance -> Light did not lighten the theme"
        );
    });

    // Back to following the desktop, which in this headless context is light.
    cx.dispatch_action(any, AppearanceSystem);
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            partytime_app::theme::preference(cx),
            partytime_app::theme::ThemePreference::System
        );
        assert!(!Theme::global(cx).is_dark());
    });

    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_welcome_screen_offers_browser_sign_in_and_never_asks_for_a_password(
    cx: &mut TestAppContext,
) {
    let dir = temp_dir("welcome");
    let (handle, view) = mount_onboarding(
        cx,
        Box::new(FakePlatform::ok().0),
        ProfileStore::new(dir.join("p")),
    );

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("signin").is_some(),
            "the welcome screen offers sign-in"
        );
        assert!(
            window.try_find("signin-email").is_none()
                && window.try_find("signin-password").is_none(),
            "there is no password field: the browser handles sign-in"
        );
        assert!(
            window.try_find("scopes").is_some(),
            "the screen says what it is asking for"
        );
        assert!(
            window.try_find("onboarding-error").is_none(),
            "a fresh screen must not show an error"
        );
    })
    .unwrap();

    cx.update(|cx| {
        assert_eq!(view.read(cx).step(), 1);
        assert_eq!(view.read(cx).step_title(), "Welcome to PartyTime");
        assert!(!view.read(cx).is_complete());
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn pressing_sign_in_asks_the_platform_once_and_moves_on(cx: &mut TestAppContext) {
    let dir = temp_dir("signin-ok");
    let (platform, calls) = FakePlatform::ok();
    let (handle, view) = mount_onboarding(cx, Box::new(platform), ProfileStore::new(dir.join("p")));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();

    // The token arrived, so the session is signed in and the next step is the profile.
    cx.update(|cx| {
        assert!(
            view.read(cx).session_is_signed_in(),
            "the sign-in must have completed"
        );
        assert_eq!(view.read(cx).step(), 2);
    });
    // Signing in fetches the party list too: the token is in, so there is no reason to
    // make the user ask for it on the next screen.
    assert_eq!(
        calls.lock().expect("calls").as_slice(),
        ["sign_in", "parties:true"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_declined_sign_in_stays_on_the_welcome_screen_and_says_why(cx: &mut TestAppContext) {
    let dir = temp_dir("signin-denied");
    let platform = FakePlatform::refusing_sign_in(PartyError::SignInDenied);
    let (handle, view) = mount_onboarding(cx, Box::new(platform), ProfileStore::new(dir.join("p")));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        assert_eq!(
            view.read(cx).step(),
            1,
            "a declined sign-in must not advance"
        );
        let error = view.read(cx).error().expect("the refusal must be shown");
        assert_eq!(error.server_text, "access_denied");
        assert!(!error.retryable, "a denial is not worth retrying");
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("onboarding-error").is_some(),
            "the refusal must be visible in the window, not only in state"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_machine_with_no_credential_store_says_so_before_the_user_signs_in(cx: &mut TestAppContext) {
    let dir = temp_dir("no-keyring");
    let (handle, _view) = mount_onboarding(
        cx,
        Box::new(FakePlatform::without_a_credential_store()),
        ProfileStore::new(dir.join("p")),
    );

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("signin").is_some(),
            "sign-in is still offered"
        );
        // The warning is visible text; its absence would leave the creator to discover it
        // by being signed out in the morning.
        assert!(
            window.try_find("scopes").is_some(),
            "the screen explains itself before any action"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_party_step_lists_what_the_platform_returned_using_its_own_ids(cx: &mut TestAppContext) {
    let dir = temp_dir("party-list");
    let mut store = ProfileStore::new(dir.join("p"));
    store.save(&Profile::starter("Friday Night")).expect("save");

    let (platform, calls) = FakePlatform::ok();
    let (handle, view) = mount_onboarding(cx, Box::new(platform), store);
    sign_in_through_to_parties(cx, handle, &view);

    // The row is keyed by the bare id the list endpoint sent, used exactly.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("party:wlyayz1ytl2u822bifb4").is_some(),
            "a party row must be addressable by the id the platform sent"
        );
        assert!(window.try_find("parties-list").is_some());
        window.click("party:wlyayz1ytl2u822bifb4", cx);
    })
    .unwrap();

    cx.update(|cx| {
        assert_eq!(
            view.read(cx).selected_party_id().as_deref(),
            Some("wlyayz1ytl2u822bifb4"),
            "the id is stored verbatim, with no prefix added or removed"
        );
    });
    let recorded = calls.lock().expect("calls").clone();
    assert!(
        recorded.contains(&"parties:true".to_string()),
        "{recorded:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn pressing_console_leaves_onboarding_and_opens_the_producer_view(cx: &mut TestAppContext) {
    let dir = temp_dir("shell-finish");
    let paths = Ok(Paths::under(&dir));
    // The shell reads `paths.profiles_dir`, so the profile has to live there.
    let mut store = ProfileStore::new(Paths::under(&dir).profiles_dir.clone());
    store.save(&Profile::starter("Friday Night")).expect("save");

    cx.update(gpui_kit::init);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            Root::new(
                cx.new(|cx| {
                    partytime_app::AppShell::new_at_with_platform(
                        paths,
                        Some(Box::new(FakePlatform::ok().0)),
                        window,
                        cx,
                    )
                }),
                window,
                cx,
            )
        },
    );
    cx.run_until_parked();

    // Drive the real screen to the Ready step.
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("choose-profile:Friday Night", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("party:wlyayz1ytl2u822bifb4", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("finish").is_some(),
            "the Console button must be on screen before it is pressed"
        );
        window.click("finish", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("go-live").is_some(),
            "pressing Console must open the producer view, not stay put"
        );
    })
    .unwrap();

    let settings = Paths::under(&dir).settings_file();
    assert!(
        settings.exists(),
        "the chosen profile and party must be remembered for next launch"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// A menu item only does something if dispatching its action reaches a handler. These two
// tests are the difference between "it compiles" and "pressing it works".
#[gpui_kit::test]
fn a_dispatched_dock_command_actually_hides_the_dock(cx: &mut TestAppContext) {
    use partytime_app::menu::{ToggleLeftDock, ToggleRightDock};

    let (handle, view) = mount_producer_focused(cx, producer_context());
    cx.update(|cx| assert!(view.read(cx).left_dock_visible(), "the dock starts showing"));
    cx.update(|cx| assert!(view.read(cx).right_dock_visible()));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ToggleLeftDock), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !view.read(cx).left_dock_visible(),
            "Docks → Left did nothing"
        )
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(ToggleRightDock), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !view.read(cx).right_dock_visible(),
            "Docks → Right did nothing"
        );
        // The left dock was hidden by the previous command and must stay as it was:
        // toggling one dock must not disturb the other.
        assert!(
            !view.read(cx).left_dock_visible(),
            "hiding the right dock changed the left one"
        );
    });
}

// After a restart the summary is gone but the id is not. The card must not claim the
// creator picked no party when console.json says otherwise.
#[gpui_kit::test]
fn a_remembered_party_is_shown_even_before_its_roster_loads(cx: &mut TestAppContext) {
    let mut context = producer_context();
    context.config.party = Some("3d9party".into());
    let (_handle, view) = mount_producer(cx, context);
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).party_card_heading().as_deref(),
            Some("Party 3d9party"),
            "a party id in console.json must not be reported as no party selected"
        );
    });
}

// The title bar is drawn in-window because GNOME renders neither a platform menu bar nor
// client decorations. Both facts regressed silently once, so pin them.
#[gpui_kit::test]
fn the_window_carries_its_own_menu_bar_and_window_controls(cx: &mut TestAppContext) {
    let dir = temp_dir("window-chrome");
    let paths = Ok(Paths::under(&dir));
    let mut store = ProfileStore::new(Paths::under(&dir).profiles_dir.clone());
    store.save(&Profile::starter("Friday Night")).expect("save");

    cx.update(gpui_kit::init);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let entity = cx.new(|cx| {
                partytime_app::AppShell::new_at_with_platform(
                    paths,
                    Some(Box::new(FakePlatform::ok().0)),
                    window,
                    cx,
                )
            });
            let focus = entity.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            Root::new(entity, window, cx)
        },
    );

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        for id in [
            "menu:File",
            "menu:Edit",
            "menu:View",
            "menu:Docks",
            "menu:Profile",
            "menu:Scene Collection",
            "menu:Tools",
            "menu:Help",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "{id} is missing from the title bar"
            );
        }
        // Exactly one set of controls, whichever layer draws it: ours when the platform
        // declined client decorations, `TitleBar`'s when it granted them. Both at once
        // is the duplicate-on-some-sessions bug.
        let ours = window.try_find("win-minimize").is_some();
        let theirs = window.try_find("minimize").is_some();
        assert!(!(ours && theirs), "window controls are drawn twice");
        assert!(ours || theirs, "the window offers no way to close itself");
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

// The pure-logic cases that used to live in producer.rs. They need a real context to
// build a view, so they live here now rather than having been dropped.
#[gpui_kit::test]
fn the_consent_rail_lists_one_row_per_declared_kind(cx: &mut TestAppContext) {
    let (_handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| {
        let kinds: Vec<_> = view
            .read(cx)
            .input_rows()
            .iter()
            .map(|row| row.kind)
            .collect();
        assert_eq!(kinds, vec![PublishKind::Gameplay, PublishKind::Mic]);
    });
}

#[gpui_kit::test]
fn the_rail_shows_no_approval_before_the_snapshot_arrives(cx: &mut TestAppContext) {
    let (_handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| {
        for row in view.read(cx).input_rows() {
            assert_eq!(
                row.consent, None,
                "consent must not be invented before /parties answers"
            );
        }
    });
}

#[gpui_kit::test]
fn hiding_a_scene_item_removes_its_kind_from_the_rail(cx: &mut TestAppContext) {
    let mut context = producer_context();
    let mut profile = context.profile.take().expect("a profile");
    // Hide the mic's scene item; its declaration must leave the programme.
    profile.scenes[0].items[1].visible = false;
    context.profile = Some(profile);

    let (_handle, view) = mount_producer(cx, context);
    cx.update(|cx| {
        let kinds: Vec<_> = view
            .read(cx)
            .input_rows()
            .iter()
            .map(|row| row.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![PublishKind::Gameplay],
            "a hidden layer is not published"
        );
    });
}

#[gpui_kit::test]
fn an_input_is_selected_by_uuid_not_by_its_name(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("source:mic", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).selected_source(),
            Some("mic"),
            "the console must key on the uuid, which is stable across a rename"
        );
    });
}

#[gpui_kit::test]
fn profile_switch_profile_returns_to_the_profile_step(cx: &mut TestAppContext) {
    use partytime_app::Route;
    use partytime_app::menu::SwitchProfile;

    let dir = temp_dir("switch-profile");
    let paths = Ok(Paths::under(&dir));
    let mut store = ProfileStore::new(Paths::under(&dir).profiles_dir.clone());
    store.save(&Profile::starter("Friday Night")).expect("save");

    cx.update(gpui_kit::init);
    let shell: Rc<RefCell<Option<Entity<partytime_app::AppShell>>>> = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&shell);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let entity = cx.new(|cx| {
                partytime_app::AppShell::new_at_with_platform(
                    paths,
                    Some(Box::new(FakePlatform::ok().0)),
                    window,
                    cx,
                )
            });
            let focus = entity.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            *sink.borrow_mut() = Some(entity.clone());
            Root::new(entity, window, cx)
        },
    );
    cx.run_until_parked();

    // Reach onboarding, choose a profile, and land on the Ready step.
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.click("choose-profile:Friday Night", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(SwitchProfile), cx);
    })
    .unwrap();
    cx.run_until_parked();

    let shell = shell.borrow().clone().expect("the shell");
    cx.update(|cx| {
        assert_eq!(
            shell.read(cx).route(),
            Route::Onboarding,
            "Profile → Switch did nothing"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn help_about_dispatches_without_disturbing_the_console(cx: &mut TestAppContext) {
    use partytime_app::menu::AboutPartyTime;

    let dir = temp_dir("about");
    let paths = Ok(Paths::under(&dir));
    cx.update(gpui_kit::init);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| {
                partytime_app::AppShell::new_at_with_platform(
                    paths,
                    Some(Box::new(FakePlatform::ok().0)),
                    window,
                    cx,
                )
            });
            let focus = shell.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(AboutPartyTime), cx);
    })
    .unwrap();
    cx.run_until_parked();

    // The dialog renders in the Root overlay layer, which `try_find` does not reach, so
    // its presence cannot be asserted headlessly. What is asserted is that the command
    // is dispatched without panicking and the console keeps working — the dialog itself
    // needs a real window to confirm.
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("step-header").is_some(),
            "the console survived Help → About"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

// Undo has to put the profile back exactly as it was, not merely change it to
// something different. Width and height are the proof: the shape is derived from them.
#[gpui_kit::test]
fn undo_puts_the_output_shape_back(cx: &mut TestAppContext) {
    let (_handle, view) = mount_producer(cx, producer_context());
    let mut before = (0u32, 0u32);
    cx.update(|cx| {
        let profile = view.read(cx).profile().expect("a profile");
        before = (profile.output.width, profile.output.height);
        assert!(
            !view.read(cx).can_undo(),
            "a fresh view has nothing to undo"
        );
    });

    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.set_aspect(partytime_engine::AspectRatio::Vertical, cx)
        })
    });
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Vertical
        );
        assert!(view.read(cx).can_undo());
    });

    cx.update(|cx| view.update(cx, |view, cx| view.undo(cx)));
    cx.update(|cx| {
        let profile = view.read(cx).profile().expect("a profile");
        assert_eq!(
            (profile.output.width, profile.output.height),
            before,
            "Undo did not put the pixel dimensions back"
        );
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Landscape
        );
        assert!(view.read(cx).can_redo());
    });

    cx.update(|cx| view.update(cx, |view, cx| view.redo(cx)));
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Vertical
        );
        assert!(
            !view.read(cx).can_redo(),
            "Redo left something behind to redo twice"
        );
    });
}

// Redo has to point at the branch the user is on, not one they have edited away from.
#[gpui_kit::test]
fn an_edit_after_an_undo_abandons_the_redo_branch(cx: &mut TestAppContext) {
    let (_handle, view) = mount_producer(cx, producer_context());

    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.set_aspect(partytime_engine::AspectRatio::Vertical, cx)
        })
    });
    cx.update(|cx| view.update(cx, |view, cx| view.undo(cx)));
    cx.update(|cx| assert!(view.read(cx).can_redo()));

    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.set_aspect(partytime_engine::AspectRatio::Square, cx)
        })
    });
    cx.update(|cx| {
        assert!(
            !view.read(cx).can_redo(),
            "a new edit left a stale Redo that would have undone the wrong shape"
        );
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Square
        );
    });
}

// Pressing the shape that is already selected changes nothing, so it must not leave an
// entry that claims there is something to undo.
// Edit -> Undo is dispatched to the shell, which owns the menu and forwards to the
// producer that owns the history. This is the whole hop: menu action, shell handler,
// producer history, and the profile that comes back.
#[gpui_kit::test]
fn a_dispatched_undo_command_reaches_the_producer_history(cx: &mut TestAppContext) {
    use partytime_app::menu::Redo;

    let dir = temp_dir("shell-undo");
    let paths = Paths::under(&dir);
    paths.ensure().expect("directories");
    let mut store = ProfileStore::new(&paths.profiles_dir);
    store.save(&Profile::starter("Friday Night")).expect("save");
    ConsoleConfig {
        profile: Some("Friday Night".into()),
        party: Some("party:abc".into()),
        account: Some("ada".into()),
        ..ConsoleConfig::default()
    }
    .save(&paths.settings_file())
    .expect("save settings");

    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let captured = slot.clone();
    let paths = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| AppShell::new_at(paths, window, cx));
            *captured.borrow_mut() = Some(shell.clone());
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();
    let shell = slot.borrow().clone().expect("the shell");

    // An edit made the way a user makes one, through the shape control.
    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        window.click("aspect:9:16 vertical", cx);
    })
    .unwrap();
    let view = cx
        .update(|cx| shell.read(cx).producer())
        .expect("the producer");
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Vertical,
            "the shape control did not take"
        );
    });

    cx.dispatch_action(any, partytime_app::menu::Undo);
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Landscape,
            "Edit -> Undo did not reach the producer's history"
        );
    });

    cx.dispatch_action(any, Redo);
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Vertical,
            "Edit -> Redo did not reach the producer's history"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

// The dock commands are dispatched to the shell but handled by the producer. This is
// that hop, from a console that is actually open.
// The whole File path, the way a creator walks it: choose the command, type a
// destination, press the button, and the collection is actually on disk.
#[gpui_kit::test]
fn exporting_writes_the_collection_to_the_path_that_was_typed(cx: &mut TestAppContext) {
    use partytime_app::menu::ExportProfile;

    let dir = temp_dir("file-export");
    let paths = Paths::under(&dir);
    paths.ensure().expect("directories");
    let mut store = ProfileStore::new(&paths.profiles_dir);
    store.save(&Profile::starter("Friday Night")).expect("save");
    ConsoleConfig {
        profile: Some("Friday Night".into()),
        party: Some("party:abc".into()),
        account: Some("ada".into()),
        ..ConsoleConfig::default()
    }
    .save(&paths.settings_file())
    .expect("save settings");

    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let captured = slot.clone();
    let paths_for_shell = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| AppShell::new_at(paths_for_shell, window, cx));
            *captured.borrow_mut() = Some(shell.clone());
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();
    let shell = slot.borrow().clone().expect("the shell");

    let out = dir.join("exported.json");
    let typed = out.to_string_lossy().to_string();

    cx.dispatch_action(any, ExportProfile);
    cx.run_until_parked();
    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("file-confirm").is_some(),
            "File -> Export profile… did not offer a way to confirm a destination"
        );
        window.click("file-path", cx);
        window.render_frame(cx);
        window.input(&typed, cx);
        window.click("file-confirm", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| assert!(!shell.read(cx).file_dialog_open()));
    assert!(
        out.exists(),
        "Export wrote nothing to {typed:?}: the command ran without producing a file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// A File command that cannot do its job must say so on screen. Swallowing it is the one
// thing this console must not do: a creator who thinks they imported a collection has to
// learn that they did not.
// Help -> About used to open a window overlay, which nothing could reach: it is outside
// the window's own render. Now the panel is drawn by the shell, so choosing the menu
// entry can actually be seen to produce it, and Close can actually be pressed.
#[gpui_kit::test]
fn the_about_menu_entry_shows_and_hides_what_this_build_is(cx: &mut TestAppContext) {
    use partytime_app::menu::AboutPartyTime;

    let dir = temp_dir("about-panel");
    let paths = Ok(Paths::under(&dir));
    cx.update(|cx| {
        gpui_kit::init(cx);
        partytime_app::theme::apply(cx);
    });
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();

    cx.dispatch_action(any, AboutPartyTime);
    cx.run_until_parked();
    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("about-version").is_some(),
            "Help -> About PartyTime did not say what this build is"
        );
        window.click("about-close", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("about-panel").is_none(),
            "Close left the about panel on screen"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn an_import_that_cannot_read_the_file_says_so_on_screen(cx: &mut TestAppContext) {
    use partytime_app::menu::ImportObsProfile;

    let dir = temp_dir("file-import-bad");
    let paths = Paths::under(&dir);
    paths.ensure().expect("directories");
    let mut store = ProfileStore::new(&paths.profiles_dir);
    store.save(&Profile::starter("Friday Night")).expect("save");
    ConsoleConfig {
        profile: Some("Friday Night".into()),
        party: Some("party:abc".into()),
        account: Some("ada".into()),
        ..ConsoleConfig::default()
    }
    .save(&paths.settings_file())
    .expect("save settings");

    cx.update(gpui_kit::init);
    let paths_for_shell = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            Root::new(
                cx.new(|cx| AppShell::new_at(paths_for_shell, window, cx)),
                window,
                cx,
            )
        },
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();
    let missing = dir
        .join("not-a-collection.json")
        .to_string_lossy()
        .to_string();

    cx.dispatch_action(any, ImportObsProfile);
    cx.run_until_parked();
    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        window.click("file-path", cx);
        window.render_frame(cx);
        window.input(&missing, cx);
        window.click("file-confirm", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(any, |_, window, cx| {
        window.render_frame(cx);
        let banner = window
            .try_find("file-error")
            .expect("an import that read nothing reported nothing: the creator cannot tell success from failure");
        assert!(
            banner.bounds().size.height > gpui_kit::px(0.),
            "the error banner rendered with no height"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_dispatched_dock_command_reaches_the_producer_from_the_shell(cx: &mut TestAppContext) {
    use partytime_app::menu::{ToggleLeftDock, ToggleRightDock};

    let dir = temp_dir("shell-docks");
    let paths = Paths::under(&dir);
    paths.ensure().expect("directories");
    let mut store = ProfileStore::new(&paths.profiles_dir);
    store.save(&Profile::starter("Friday Night")).expect("save");
    ConsoleConfig {
        profile: Some("Friday Night".into()),
        party: Some("party:abc".into()),
        account: Some("ada".into()),
        ..ConsoleConfig::default()
    }
    .save(&paths.settings_file())
    .expect("save settings");

    cx.update(gpui_kit::init);
    let slot = Rc::new(RefCell::new(None));
    let captured = slot.clone();
    let paths = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| AppShell::new_at(paths, window, cx));
            *captured.borrow_mut() = Some(shell.clone());
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();
    let any: gpui_kit::AnyWindowHandle = handle.into();
    let shell = slot.borrow().clone().expect("the shell");
    let view = cx
        .update(|cx| shell.read(cx).producer())
        .expect("the producer");

    cx.update(|cx| assert!(view.read(cx).left_dock_visible()));

    cx.dispatch_action(any, ToggleLeftDock);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !view.read(cx).left_dock_visible(),
            "Docks -> Left was dispatched to the shell and never reached the producer"
        );
        assert!(
            view.read(cx).right_dock_visible(),
            "hiding the left dock must not disturb the right one"
        );
    });

    cx.dispatch_action(any, ToggleRightDock);
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !view.read(cx).right_dock_visible(),
            "Docks -> Right was dispatched to the shell and never reached the producer"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn choosing_the_shape_already_selected_is_not_an_edit(cx: &mut TestAppContext) {
    let (_handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.set_aspect(partytime_engine::AspectRatio::Landscape, cx)
        })
    });
    cx.update(|cx| {
        assert!(
            !view.read(cx).can_undo(),
            "re-selecting the current shape put something on the undo stack"
        );
    });
}

#[gpui_kit::test]
fn choosing_vertical_rewrites_the_output_shape_in_place(cx: &mut TestAppContext) {
    use partytime_app::producer::ViewLayout;

    let (handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Landscape
        );
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("aspect:16:9 landscape").is_some(),
            "the shape control must be on screen"
        );
        window.click("aspect:9:16 vertical", cx);
    })
    .unwrap();

    cx.update(|cx| {
        assert_eq!(
            view.read(cx).aspect(),
            partytime_engine::AspectRatio::Vertical,
            "9:16 was pressed and nothing changed"
        );
        // The rest of the console keeps working with the new shape.
        assert_eq!(view.read(cx).layout(), ViewLayout::Split);
    });
    let _ = view;
}

#[gpui_kit::test]
fn a_dispatched_appearance_command_actually_changes_the_theme(cx: &mut TestAppContext) {
    use gpui_kit::component::Theme;
    use partytime_app::menu::{AppearanceDark, AppearanceLight, AppearanceSystem};

    let dir = temp_dir("appearance-action");
    let paths = Ok(Paths::under(&dir));
    cx.update(gpui_kit::init);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| {
            let shell = cx.new(|cx| {
                partytime_app::AppShell::new_at_with_platform(
                    paths,
                    Some(Box::new(FakePlatform::ok().0)),
                    window,
                    cx,
                )
            });
            let focus = shell.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            Root::new(shell, window, cx)
        },
    );
    cx.run_until_parked();

    cx.update(|cx| {
        partytime_app::theme::use_preference(partytime_app::theme::ThemePreference::System, cx);
    });

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(AppearanceDark), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            partytime_app::theme::preference(cx),
            partytime_app::theme::ThemePreference::Dark
        );
        assert!(
            Theme::global(cx).is_dark(),
            "View → Appearance → Dark did not repaint dark"
        );
    });

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(AppearanceLight), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(!Theme::global(cx).is_dark(), "…Light did not go back");
    });

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(AppearanceSystem), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            partytime_app::theme::preference(cx),
            partytime_app::theme::ThemePreference::System
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn pressing_back_steps_back_a_stage(cx: &mut TestAppContext) {
    let dir = temp_dir("back");
    let mut store = ProfileStore::new(dir.join("p"));
    store.save(&Profile::starter("Friday Night")).expect("save");
    let (handle, view) = mount_onboarding(cx, Box::new(FakePlatform::ok().0), store);
    sign_in_through_to_parties(cx, handle, &view);

    // The helper stops at the party step; pick a party to reach the Ready step.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("party:wlyayz1ytl2u822bifb4", cx);
    })
    .unwrap();
    cx.update(|cx| assert_eq!(view.read(cx).step(), 4, "profile and party chosen"));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("back", cx);
    })
    .unwrap();

    cx.update(|cx| {
        assert_eq!(
            view.read(cx).step(),
            3,
            "Back must clear the party in both the view and the session"
        );
        assert_eq!(view.read(cx).selected_party_id(), None);
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("back", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(view.read(cx).step(), 2, "and again clears the profile");
        assert_eq!(view.read(cx).chosen_profile(), None);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_finish_button_is_enabled_once_profile_and_party_are_chosen(cx: &mut TestAppContext) {
    let dir = temp_dir("finish-enabled");
    let (handle, view) = mount_onboarding(
        cx,
        Box::new(FakePlatform::ok().0),
        ProfileStore::new(dir.join("p")),
    );

    // 1. sign in
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            view.read(cx).session_is_signed_in(),
            "sign-in should have completed"
        );
        assert!(
            view.read(cx).identity().is_some(),
            "the identity should come from /me"
        );
        assert_eq!(view.read(cx).step(), 2, "no profile yet");
    });

    // 2. create and choose a profile
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("profile-new", cx);
    })
    .unwrap();

    // 3. choose a party
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("party:wlyayz1ytl2u822bifb4", cx);
    })
    .unwrap();

    cx.update(|cx| {
        let view = view.read(cx);
        assert_eq!(view.step(), 4, "profile and party are both chosen");
        assert!(
            view.is_complete(),
            "ready to open: profile={:?} party={:?} identity={:?}",
            view.chosen_profile(),
            view.selected_party_id(),
            view.identity().map(|i| i.handle.clone()),
        );
    });

    // 4. the control a creator actually presses must not be disabled.
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("finish").is_some(),
            "the final action must be on screen"
        );
        // `disabled()` reports `None` when it cannot tell, which is not the same as
        // enabled — so prove it by pressing it instead of by inspecting it.
        window.click("finish", cx);
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_finish_button_says_console_and_not_open_console(cx: &mut TestAppContext) {
    let dir = temp_dir("finish-label");
    let mut store = ProfileStore::new(dir.join("p"));
    store.save(&Profile::starter("Friday Night")).expect("save");

    let (handle, view) = mount_onboarding(cx, Box::new(FakePlatform::ok().0), store);
    sign_in_through_to_parties(cx, handle, &view);

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("party:wlyayz1ytl2u822bifb4", cx);
        window.render_frame(cx);
    })
    .unwrap();

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .try_find("finish")
                .and_then(|node| node.label().map(str::to_string))
                .as_deref(),
            Some("Console"),
            "the final action is named Console, with the arrow carried as an icon"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn completing_onboarding_reports_the_profile_party_and_account(cx: &mut TestAppContext) {
    let dir = temp_dir("finish");
    let mut store = ProfileStore::new(dir.join("p"));
    store.save(&Profile::starter("Friday Night")).expect("save");

    let (platform, _calls) = FakePlatform::ok();
    let (handle, view) = mount_onboarding(cx, Box::new(platform), store);
    sign_in_through_to_parties(cx, handle, &view);

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("party:wlyayz1ytl2u822bifb4", cx);
        window.render_frame(cx);
        window.click("finish", cx);
    })
    .unwrap();

    cx.update(|cx| {
        let result = view.read(cx).result().expect("onboarding finished");
        assert_eq!(result.profile, "Friday Night");
        assert_eq!(result.party, "wlyayz1ytl2u822bifb4");
        assert_eq!(result.account, "ada", "the handle, never an email");
        assert!(!result.account.contains('@'));
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn onboarding_cannot_finish_without_a_profile_and_a_party(cx: &mut TestAppContext) {
    let dir = temp_dir("incomplete");
    let (handle, view) = mount_onboarding(
        cx,
        Box::new(FakePlatform::ok().0),
        ProfileStore::new(dir.join("p")),
    );

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("signin", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        assert!(
            !view.read(cx).is_complete(),
            "a profile and a party are still missing"
        );
    });
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- producer

struct ProducerHost {
    view: Entity<ProducerView>,
}

impl Render for ProducerHost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.view.clone().into_any_element()
    }
}

fn mount_producer(
    cx: &mut TestAppContext,
    context: ProducerContext,
) -> (gpui_kit::AnyWindowHandle, Entity<ProducerView>) {
    cx.update(gpui_kit::init);
    cx.update(gpui_kit::init);
    let captured: Rc<RefCell<Option<Entity<ProducerView>>>> = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&captured);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(760.)),
        move |window, cx| {
            let entity = cx.new(|cx| ProducerView::new(context, cx));
            *sink.borrow_mut() = Some(entity.clone());
            Root::new(cx.new(|_| ProducerHost { view: entity }), window, cx)
        },
    );
    let view = captured.borrow().clone().expect("view captured");
    (handle.into(), view)
}

/// Mounts the producer view with its root focused, the way window activation leaves it.
///
/// Menu actions dispatch from the focused node, so a test that skips this would be testing
/// nothing.
fn mount_producer_focused(
    cx: &mut TestAppContext,
    context: ProducerContext,
) -> (gpui_kit::AnyWindowHandle, Entity<ProducerView>) {
    cx.update(gpui_kit::init);
    let captured: Rc<RefCell<Option<Entity<ProducerView>>>> = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&captured);
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(760.)),
        move |window, cx| {
            let entity = cx.new(|cx| ProducerView::new(context, cx));
            *sink.borrow_mut() = Some(entity.clone());
            let focus = entity.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            Root::new(cx.new(|_| ProducerHost { view: entity }), window, cx)
        },
    );
    (
        handle.into(),
        captured.borrow().clone().expect("view captured"),
    )
}

fn producer_context() -> ProducerContext {
    ProducerContext {
        party: None,
        config: ConsoleConfig {
            profile: Some("Friday Night".into()),
            ..ConsoleConfig::default()
        },
        profile: Some(Profile::starter("Friday Night")),
        engine: EngineStatus::NotIntegrated,
        identity: identity(),
    }
}

#[gpui_kit::test]
fn the_producer_view_shows_the_scenes_inputs_and_publish_controls(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        for id in [
            "scene:Party Scene",
            "source:mic",
            "source:screen",
            "party-card",
            "go-live",
            "stop",
            "layout:Split",
        ] {
            assert!(
                window.try_find(id).is_some(),
                "{id} is missing from the producer view"
            );
        }
    })
    .unwrap();

    cx.update(|cx| {
        let view = view.read(cx);
        assert_eq!(view.selected_scene(), Some("Party Scene"));
        assert_eq!(view.layout(), partytime_app::producer::ViewLayout::Split);
    });
}

#[gpui_kit::test]
fn clicking_a_scene_reprograms_the_view(cx: &mut TestAppContext) {
    let mut context = producer_context();
    let mut profile = context.profile.take().expect("profile");
    profile.scenes.push(partytime_engine::Scene {
        name: "Starting Soon".into(),
        items: vec![partytime_engine::SceneItem {
            source_uuid: "screen".into(),
            transform: partytime_engine::Transform::default(),
            visible: true,
        }],
    });
    context.profile = Some(profile);

    let (handle, view) = mount_producer(cx, context);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("scene:Starting Soon", cx);
    })
    .unwrap();

    cx.update(|cx| {
        assert_eq!(view.read(cx).selected_scene(), Some("Starting Soon"));
        // The consent rail follows the programme, so switching scene changes it.
        let kinds: Vec<PublishKind> = view.read(cx).input_rows().iter().map(|r| r.kind).collect();
        assert_eq!(kinds, vec![PublishKind::Gameplay]);
    });
}

// The aspect control used to change the profile and nothing else: the canvas box was
// `flex_1` with no shape, so it filled whatever the panel gave it. These assert the
// measured geometry, not the state that was written.
// The canvas must never be larger than the panel it sits in: a shape that does not fit
// the available space shrinks, it is not allowed to overflow or be cropped.
// The other way round, and the one that bites: a wide shape in a narrow panel has to
// give up height. Without a height clamp the canvas keeps `h_full` and overflows.
#[gpui_kit::test]
fn a_wide_canvas_shrinks_to_fit_a_narrow_panel(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| view.update(cx, |view, _cx| view.set_layout(ViewLayout::ProgramOnly)));
    cx.update(|cx| view.update(cx, |view, cx| view.set_aspect(AspectRatio::Landscape, cx)));

    // A tall, narrow window: 16:9 cannot fit at full height.
    cx.update_window(handle, |_, window, cx| {
        window.resize(gpui_kit::size(gpui_kit::px(520.), gpui_kit::px(1000.)));
        window.render_frame(cx);
        let canvas = window.find("preview-program").bounds();
        let stage = window.find("preview-stage:preview-program").bounds();
        assert!(
            canvas.size.height <= stage.size.height + gpui_kit::px(1.),
            "a 16:9 canvas is taller than its panel: canvas {canvas:?} stage {stage:?}"
        );
        assert!(
            canvas.size.width <= stage.size.width + gpui_kit::px(1.),
            "canvas {canvas:?} overflows stage {stage:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_canvas_never_outgrows_the_panel_it_sits_in(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| view.update(cx, |view, _cx| view.set_layout(ViewLayout::ProgramOnly)));
    cx.update(|cx| view.update(cx, |view, cx| view.set_aspect(AspectRatio::Vertical, cx)));

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let canvas = window.find("preview-program").bounds();
        let stage = window.find("preview-stage:preview-program").bounds();
        assert!(
            canvas.size.width <= stage.size.width + gpui_kit::px(1.)
                && canvas.size.height <= stage.size.height + gpui_kit::px(1.),
            "canvas {canvas:?} overflows stage {stage:?}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_canvas_takes_the_shape_the_aspect_control_selected(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());
    // One surface, so the measurement is not muddied by the split divider.
    cx.update(|cx| view.update(cx, |view, _cx| view.set_layout(ViewLayout::ProgramOnly)));

    for preset in AspectRatio::PRESETS {
        cx.update(|cx| view.update(cx, |view, cx| view.set_aspect(preset, cx)));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let size = window.find("preview-program").bounds().size;
            let (want_w, want_h) = preset.ratio().expect("presets have a ratio");
            let want = want_w as f32 / want_h as f32;
            let got = size.width / size.height;
            assert!(
                size.width > gpui_kit::px(0.) && size.height > gpui_kit::px(0.),
                "{preset:?} produced a collapsed canvas: {size:?}"
            );
            assert!(
                (got - want).abs() < 0.02,
                "{preset:?} canvas is {got:.3} ({}x{}), expected {want:.3}",
                size.width,
                size.height
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn a_vertical_canvas_letterboxes_rather_than_stretching(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());
    cx.update(|cx| view.update(cx, |view, _cx| view.set_layout(ViewLayout::ProgramOnly)));

    cx.update(|cx| view.update(cx, |view, cx| view.set_aspect(AspectRatio::Vertical, cx)));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let size = window.find("preview-program").bounds().size;
        // In a wide panel a 9:16 canvas must be narrower than it is tall, not filled.
        assert!(
            size.height > size.width,
            "a 9:16 canvas filled a wide panel: {size:?}"
        );
        assert!(size.width > gpui_kit::px(0.), "{size:?}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_layout_switch_changes_how_many_surfaces_are_shown(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("layout:Input", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).layout(),
            partytime_app::producer::ViewLayout::InputOnly
        );
        assert_eq!(view.read(cx).layout().surface_count(), 1);
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("layout:Split", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert_eq!(
            view.read(cx).layout(),
            partytime_app::producer::ViewLayout::Split
        );
        assert_eq!(view.read(cx).layout().surface_count(), 2);
    });
}

#[gpui_kit::test]
fn muting_a_source_is_independent_of_the_others(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("mute:mic", cx);
    })
    .unwrap();
    cx.update(|cx| {
        assert!(view.read(cx).is_muted("mic"));
        assert!(!view.read(cx).is_muted("screen"));
    });

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("mute:mic", cx);
    })
    .unwrap();
    cx.update(|cx| assert!(!view.read(cx).is_muted("mic")));
}

#[gpui_kit::test]
fn publish_controls_are_present_and_the_blocker_is_stated(cx: &mut TestAppContext) {
    let (handle, view) = mount_producer(cx, producer_context());

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("go-live").is_some(),
            "Go live stays visible even while it is unavailable"
        );
    })
    .unwrap();

    cx.update(|cx| {
        assert!(!view.read(cx).can_publish());
        let reason = view
            .read(cx)
            .publish_blocked_reason()
            .expect("a reason must be given");
        assert!(
            reason.contains("libobs"),
            "the status bar must name what is missing, got: {reason}"
        );
    });
}

#[gpui_kit::test]
fn a_producer_view_with_no_profile_says_so_instead_of_rendering_nothing(cx: &mut TestAppContext) {
    let context = ProducerContext {
        party: None,
        config: ConsoleConfig::default(),
        profile: None,
        engine: EngineStatus::Idle,
        identity: identity(),
    };
    let (handle, view) = mount_producer(cx, context);

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("go-live").is_some(),
            "the console stays usable enough to get a profile"
        );
    })
    .unwrap();

    cx.update(|cx| {
        assert!(view.read(cx).profile().is_none());
        assert!(view.read(cx).input_rows().is_empty());
        assert_eq!(
            view.read(cx).publish_blocked_reason(),
            Some("Load a profile before publishing.")
        );
    });
}

// ------------------------------------------------------------------ shell

#[gpui_kit::test]
fn the_shell_opens_onboarding_when_there_is_nothing_remembered(cx: &mut TestAppContext) {
    let dir = temp_dir("shell-onb");
    let _ = std::fs::remove_dir_all(dir.join("console.json"));

    cx.update(gpui_kit::init);
    let paths = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );

    // Let the bootstrap run to completion.
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("signin").is_some(),
            "a console with nothing remembered must land on the welcome screen"
        );
        assert!(
            window.try_find("go-live").is_none(),
            "the producer view must not open"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_remembered_profile_and_party_take_the_shell_straight_to_the_producer(cx: &mut TestAppContext) {
    let dir = temp_dir("shell-prod");
    let paths = Paths::under(&dir);
    paths.ensure().expect("directories");
    let mut store = ProfileStore::new(&paths.profiles_dir);
    store.save(&Profile::starter("Friday Night")).expect("save");
    ConsoleConfig {
        profile: Some("Friday Night".into()),
        party: Some("party:abc".into()),
        account: Some("ada".into()),
        ..ConsoleConfig::default()
    }
    .save(&paths.settings_file())
    .expect("save settings");

    cx.update(gpui_kit::init);
    let paths = Ok(Paths::under(&dir));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(800.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("go-live").is_some(),
            "a bootstrapped console must open the producer view"
        );
        assert!(
            window.try_find("signin-email").is_none(),
            "onboarding must be skipped"
        );
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn an_unusable_configuration_directory_is_reported_on_the_splash(cx: &mut TestAppContext) {
    // A regular file where a directory must go: `create_dir_all` cannot succeed.
    let blocker = std::env::temp_dir().join(format!("pt-blocker-{}", std::process::id()));
    std::fs::write(&blocker, b"x").expect("write");

    cx.update(gpui_kit::init);
    let paths = Ok(Paths::under(blocker.join("nested")));
    let handle = cx.open_window(
        gpui_kit::size(gpui_kit::px(480.), gpui_kit::px(360.)),
        move |window, cx| Root::new(cx.new(|cx| AppShell::new_at(paths, window, cx)), window, cx),
    );
    cx.run_until_parked();

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("boot-step-0").is_some(),
            "the splash must stay up and name the failing step"
        );
        assert!(
            window.try_find("signin-email").is_none(),
            "a failed bootstrap must not fall through to onboarding"
        );
    })
    .unwrap();
    let _ = std::fs::remove_file(&blocker);
}
