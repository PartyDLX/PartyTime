//! The producer view: the publishing console.
//!
//! Three resizable regions — the local setup on the left, the video surfaces in the
//! middle, the party on the right — over a status bar that is the single source of
//! truth for publish state.
//!
//! Everything shown here is real state read from the profile and the engine status. The
//! video surfaces are honest about having nothing to show until the libobs engine is
//! linked (spike S2); they never render a placeholder that looks like a live feed.

use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::assets::IconName;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    resizable::{h_resizable, resizable_panel, v_resizable},
    status_bar::StatusBar,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window, div, px,
};
use studio_engine::{AudioChannel, EngineStatus, InputKind, Profile, PublishKind, Scene, Source};
use studio_party::ConsentState;

use crate::{
    appearance::AppearanceMenu,
    menu::{ToggleLeftDock, ToggleRightDock},
    paths::ConsoleConfig,
    theme,
};

/// How much of the video surface is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLayout {
    /// Program preview above, live feed below.
    Split,
    /// Program preview only.
    ProgramOnly,
    /// The raw input only.
    InputOnly,
}

impl ViewLayout {
    /// Every layout, in the order the switch offers them.
    pub const ALL: [Self; 3] = [Self::Split, Self::ProgramOnly, Self::InputOnly];

    /// The label on the layout switch.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Split => "Split",
            Self::ProgramOnly => "Preview",
            Self::InputOnly => "Input",
        }
    }

    /// A short explanation, so the abbreviations are not a puzzle.
    #[must_use]
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Split => "Program preview and live feed",
            Self::ProgramOnly => "Program preview only",
            Self::InputOnly => "Input video only",
        }
    }

    /// How many video surfaces this layout shows.
    #[must_use]
    pub const fn surface_count(self) -> usize {
        match self {
            Self::Split => 2,
            Self::ProgramOnly | Self::InputOnly => 1,
        }
    }
}

/// What the producer view was given when it was created.
#[derive(Debug, Clone)]
pub struct ProducerContext {
    /// The party chosen during onboarding, as the platform described it.
    ///
    /// `None` means no party was chosen — not that signing in failed. The distinction
    /// matters: the console used to say "sign in" on a screen the user had already signed
    /// in on their way past.
    pub party: Option<studio_party::PartySummary>,
    /// Remembered console settings.
    pub config: ConsoleConfig,
    /// The loaded profile, if one is.
    pub profile: Option<Profile>,
    /// What the media engine is doing.
    pub engine: EngineStatus,
    /// Who is publishing.
    pub identity: studio_party::Identity,
}

/// One row of the consent rail.
///
/// `consent` is `None` until the room snapshot arrives; the rail says so rather than
/// inventing an approval state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRow {
    /// The kind declared by a source in the program.
    pub kind: PublishKind,
    /// The source that declares it.
    pub source_name: SharedString,
    /// The party's decision, once known.
    pub consent: Option<ConsentState>,
}

/// The producer view.
///
/// Mutators take `&mut self` only; the caller notifies. That keeps every rule in this
/// type testable without a window, which is the point of putting them here rather than
/// in the click handlers.
pub struct ProducerView {
    context: ProducerContext,
    layout: ViewLayout,
    selected_scene: Option<SharedString>,
    selected_source: Option<String>,
    muted: Vec<String>,
    left_dock: bool,
    right_dock: bool,
    /// Actions dispatched from the menu bar are routed from the focused node, so the
    /// root has to be focusable and focused — an `ElementId` alone is not reachable.
    focus: gpui_kit::FocusHandle,
    /// The profile as it was before each edit, oldest first. Only profile edits are
    /// undoable: which dock is open or which row is selected is view state, and OBS
    /// does not put that in its history either.
    past: Vec<Option<Profile>>,
    /// Edits that were undone and can be put back with Edit → Redo.
    future: Vec<Option<Profile>>,
}

impl ProducerView {
    /// Creates the view from its context.
    #[must_use]
    pub fn new(context: ProducerContext, cx: &mut Context<Self>) -> Self {
        let selected_scene = context
            .profile
            .as_ref()
            .and_then(|profile| profile.scenes.first())
            .map(|scene| scene.name.clone().into());
        Self {
            context,
            layout: ViewLayout::Split,
            selected_scene,
            selected_source: None,
            muted: Vec::new(),
            left_dock: true,
            right_dock: true,
            focus: cx.focus_handle(),
            past: Vec::new(),
            future: Vec::new(),
        }
    }

    /// The profile the view is rendering, if one loaded.
    #[must_use]
    pub fn profile(&self) -> Option<&Profile> {
        self.context.profile.as_ref()
    }

    /// The scene that is currently programmed.
    #[must_use]
    pub fn selected_scene(&self) -> Option<&str> {
        self.selected_scene.as_deref()
    }

    /// The input that is currently selected.
    #[must_use]
    pub fn selected_source(&self) -> Option<&str> {
        self.selected_source.as_deref()
    }

    /// The layout currently on screen.
    #[must_use]
    pub const fn layout(&self) -> ViewLayout {
        self.layout
    }

    /// The root's focus handle.
    ///
    /// Exposed because focusing is what makes the root reachable by a dispatched
    /// action; a test has to do it the way window activation does.
    #[must_use]
    pub fn focus_handle(&self) -> &gpui_kit::FocusHandle {
        &self.focus
    }

    /// Whether the left dock is showing.
    #[must_use]
    pub const fn left_dock_visible(&self) -> bool {
        self.left_dock
    }

    /// Whether the right dock is showing.
    #[must_use]
    pub const fn right_dock_visible(&self) -> bool {
        self.right_dock
    }

    /// Shows or hides the left dock.
    pub fn toggle_left_dock(&mut self, cx: &mut Context<Self>) {
        self.left_dock = !self.left_dock;
        cx.notify();
    }

    /// The output shape currently selected.
    #[must_use]
    pub fn aspect(&self) -> studio_engine::AspectRatio {
        self.context
            .profile
            .as_ref()
            .map_or(studio_engine::AspectRatio::default(), |profile| {
                profile.output.aspect
            })
    }

    /// Switches the output shape, keeping the pixel count.
    pub fn set_aspect(&mut self, preset: studio_engine::AspectRatio, cx: &mut Context<Self>) {
        let Some(current) = self.context.profile.as_ref() else {
            return;
        };
        // Choosing the shape that is already selected is not an edit, so it must not
        // put anything on the undo stack.
        if current.output.aspect == preset {
            return;
        }
        let (width, height) = preset.apply(current.output.width, current.output.height);
        self.record();
        if let Some(profile) = self.context.profile.as_mut() {
            profile.output.aspect = preset;
            profile.output.width = width;
            profile.output.height = height;
        }
        cx.notify();
    }

    /// Remembers the profile as it is now, before an edit changes it.
    fn record(&mut self) {
        self.past.push(self.context.profile.clone());
        // An edit made after an undo abandons the undone branch rather than keeping it
        // alive behind a Redo that would undo the wrong thing.
        self.future.clear();
    }

    /// Whether Edit → Undo has anything to put back.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether Edit → Redo has anything to put back.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Edit → Undo: restores the profile as it was before the last edit.
    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.past.pop() else {
            return;
        };
        self.future.push(self.context.profile.clone());
        self.context.profile = previous;
        cx.notify();
    }

    /// Edit → Redo: puts back the edit Undo took away.
    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let Some(next) = self.future.pop() else {
            return;
        };
        self.past.push(self.context.profile.clone());
        self.context.profile = next;
        cx.notify();
    }

    /// Shows or hides the right dock.
    pub fn toggle_right_dock(&mut self, cx: &mut Context<Self>) {
        self.right_dock = !self.right_dock;
        cx.notify();
    }

    /// Switches the video layout.
    pub fn set_layout(&mut self, layout: ViewLayout) {
        self.layout = layout;
    }

    /// Selects a scene by name.
    pub fn select_scene(&mut self, name: impl Into<SharedString>) {
        self.selected_scene = Some(name.into());
    }

    /// Selects a source by uuid.
    pub fn select_source(&mut self, uuid: impl Into<String>) {
        self.selected_source = Some(uuid.into());
    }

    /// Whether a source is muted in this view.
    #[must_use]
    pub fn is_muted(&self, uuid: &str) -> bool {
        self.muted.iter().any(|id| id == uuid)
    }

    /// Toggles a source's mute.
    pub fn toggle_mute(&mut self, uuid: &str) {
        match self.muted.iter().position(|id| id == uuid) {
            Some(index) => {
                self.muted.remove(index);
            }
            None => self.muted.push(uuid.to_string()),
        }
    }

    /// The consent rail rows for the program scene.
    ///
    /// One row per declared kind, because the party approves kinds, not sources.
    #[must_use]
    pub fn input_rows(&self) -> Vec<InputRow> {
        let Some(profile) = &self.context.profile else {
            return Vec::new();
        };
        let scene_name = self.selected_scene.as_deref().unwrap_or_default();
        profile
            .declared_kinds_in(scene_name)
            .into_iter()
            .map(|kind| {
                let source_name = profile
                    .scene(scene_name)
                    .map(|scene: &Scene| {
                        scene
                            .items
                            .iter()
                            .filter_map(|item| profile.source(&item.source_uuid))
                            .find(|source| source.kind == kind)
                            .map_or_else(String::new, |source| source.name.clone())
                    })
                    .unwrap_or_default();
                InputRow {
                    kind,
                    source_name: source_name.into(),
                    consent: None,
                }
            })
            .collect()
    }

    /// Whether the publish controls may be used.
    #[must_use]
    pub fn can_publish(&self) -> bool {
        self.context.engine.can_publish() && self.selected_scene.is_some()
    }

    /// Why publishing is unavailable, or `None` when it is available.
    #[must_use]
    pub fn publish_blocked_reason(&self) -> Option<&'static str> {
        self.context.engine.blocked_reason()
    }
}

impl ProducerView {
    fn toggle_left_dock_action(
        &mut self,
        _: &ToggleLeftDock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_left_dock(cx);
    }

    fn toggle_right_dock_action(
        &mut self,
        _: &ToggleRightDock,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_right_dock(cx);
    }
}

impl Render for ProducerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let profile = self.context.profile.clone();

        v_flex()
            .id("producer-root")
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            .on_action(cx.listener(Self::toggle_left_dock_action))
            .on_action(cx.listener(Self::toggle_right_dock_action))
            .child(
                div().flex_1().min_h_0().child(
                    h_resizable("producer-columns")
                        .child(
                            resizable_panel()
                                .size(px(288.))
                                .size_range(px(220.)..px(420.))
                                .visible(self.left_dock)
                                .child(self.local_dock(profile.as_ref(), cx)),
                        )
                        .child(resizable_panel().child(self.video_region(cx)))
                        .child(
                            resizable_panel()
                                .size(px(320.))
                                .size_range(px(260.)..px(460.))
                                .visible(self.right_dock)
                                .child(self.party_dock(cx)),
                        ),
                ),
            )
            .child(self.status_bar(cx))
    }
}

impl ProducerView {
    fn local_dock(&self, profile: Option<&Profile>, cx: &mut Context<Self>) -> AnyElement {
        v_resizable("producer-local")
            .child(
                resizable_panel()
                    .size(px(200.))
                    .size_range(px(120.)..px(320.))
                    .child(section("Scenes", scene_list(profile, self, cx), cx)),
            )
            .child(
                resizable_panel()
                    .size(px(260.))
                    .size_range(px(180.)..px(420.))
                    .child(section("Inputs", source_list(profile, self, cx), cx)),
            )
            .child(resizable_panel().child(section("Audio mixer", mixer(profile, self, cx), cx)))
            .into_any_element()
    }

    fn video_region(&self, cx: &mut Context<Self>) -> AnyElement {
        let switch = h_flex()
            .gap_1()
            .children(ViewLayout::ALL.into_iter().map(|layout| {
                Button::new(format!("layout:{}", layout.label()))
                    .small()
                    .label(layout.label())
                    .tooltip(layout.tooltip())
                    .when(layout == self.layout, Button::primary)
                    .when(layout != self.layout, Button::ghost)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_layout(layout);
                        cx.notify();
                    }))
            }));

        let header = h_flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        self.selected_scene
                            .clone()
                            .unwrap_or_else(|| "No scene".into()),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .children(
                        studio_engine::AspectRatio::PRESETS
                            .into_iter()
                            .map(|preset| {
                                let active = self.aspect() == preset;
                                Button::new(format!("aspect:{}", preset.label()))
                                    .small()
                                    .label(short_label(preset))
                                    .tooltip(preset.label())
                                    .when(active, Button::primary)
                                    .when(!active, Button::ghost)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_aspect(preset, cx)
                                    }))
                            }),
                    ),
            )
            .child(switch);

        let surfaces: AnyElement = match self.layout {
            ViewLayout::Split => v_resizable("producer-video")
                .child(
                    // Both panels start equal and can each be dragged almost to the
                    // full extent, so the handle gives a proportion rather than fighting
                    // a fixed cap on one side.
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(120.)..px(1600.))
                        .child(video_surface(
                            "preview-program",
                            "Program preview",
                            self.canvas_ratio(),
                            cx,
                        )),
                )
                .child(
                    resizable_panel()
                        .size(px(340.))
                        .size_range(px(120.)..px(1600.))
                        .child(video_surface(
                            "preview-live",
                            "Live feed",
                            self.canvas_ratio(),
                            cx,
                        )),
                )
                .into_any_element(),
            ViewLayout::ProgramOnly => video_surface(
                "preview-program",
                "Program preview",
                self.canvas_ratio(),
                cx,
            )
            .into_any_element(),
            ViewLayout::InputOnly => {
                video_surface("preview-input", "Input", self.canvas_ratio(), cx).into_any_element()
            }
        };

        v_flex()
            .size_full()
            .min_w_0()
            .child(header)
            .child(div().size_full().min_h_0().child(surfaces))
            .into_any_element()
    }

    /// The shape the canvas should take, as width over height.
    ///
    /// A preset is its own ratio. `Custom` falls back to the profile's own output
    /// dimensions, which is the only honest answer: the creator has said the shape is
    /// whatever they typed, so the canvas follows that and not a guess.
    fn canvas_ratio(&self) -> f32 {
        if let Some((width, height)) = self.aspect().ratio() {
            return width as f32 / height as f32;
        }
        self.context
            .profile
            .as_ref()
            .filter(|profile| profile.output.height > 0)
            .map_or(16.0 / 9.0, |profile| {
                profile.output.width as f32 / profile.output.height as f32
            })
    }

    fn party_dock(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_profile = self.context.profile.is_some();
        v_flex()
            .id("party-dock-scroll")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .gap_4()
            .p_3()
            .child(party_card(
                self.context.party.as_ref(),
                self.context.config.party.as_deref(),
                cx,
            ))
            .child(section(
                "Party inputs",
                input_rail(self.input_rows(), cx),
                cx,
            ))
            .when(!has_profile, |this| {
                this.child(empty_note(
                    "No profile loaded",
                    "Choose a profile in onboarding to declare inputs.",
                    cx,
                ))
            })
            .into_any_element()
    }

    /// The heading the party card shows.
    ///
    /// `None` when the platform has described the party and the card shows its title;
    /// otherwise the fallback wording, which still names a remembered party.
    #[must_use]
    pub fn party_card_heading(&self) -> Option<String> {
        self.context
            .party
            .as_ref()
            .map(|party| party.title.clone())
            .or_else(|| Some(party_card_fallback(self.context.config.party.as_deref()).0))
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let blocked = self.publish_blocked_reason();
        let go_live = Button::new("go-live")
            .small()
            .icon(IconName::Radio)
            .label("Go live")
            .disabled(!self.can_publish())
            .primary();
        let stop = Button::new("stop")
            .small()
            .icon(IconName::Square)
            .label("Stop")
            .disabled(!self.can_publish())
            .danger();

        let mut bar = StatusBar::new().left(self.context.engine.summary());
        bar = bar.left(
            AppearanceMenu {
                preference: theme::preference(cx),
            }
            .into_any_element(),
        );
        if let Some(reason) = blocked {
            bar = bar.left(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::Info).xsmall())
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(reason.to_string()),
                    ),
            );
        }
        bar.child(
            self.context
                .config
                .profile
                .clone()
                .unwrap_or_else(|| "No profile".into()),
        )
        .right(go_live)
        .right(stop)
        .into_any_element()
    }
}

fn section(title: &str, body: AnyElement, cx: &mut Context<ProducerView>) -> AnyElement {
    v_flex()
        .size_full()
        .min_h_0()
        .child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(title.to_string()),
        )
        .child(
            div()
                .id("scroll-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
        .into_any_element()
}

fn scene_list(
    profile: Option<&Profile>,
    view: &ProducerView,
    cx: &mut Context<ProducerView>,
) -> AnyElement {
    let Some(profile) = profile.filter(|profile| !profile.scenes.is_empty()) else {
        return empty_note("No scenes", "Load a profile to add scenes.", cx);
    };
    v_flex()
        .gap_1()
        .p_2()
        .children(profile.scenes.iter().map(|scene| {
            let name = scene.name.clone();
            let active = view.selected_scene() == Some(name.as_str());
            Button::new(format!("scene:{name}"))
                .label(name.clone())
                .when(active, Button::primary)
                .when(!active, Button::ghost)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_scene(name.clone());
                    cx.notify();
                }))
                .into_any_element()
        }))
        .into_any_element()
}

fn source_list(
    profile: Option<&Profile>,
    view: &ProducerView,
    cx: &mut Context<ProducerView>,
) -> AnyElement {
    let Some(profile) = profile.filter(|profile| !profile.sources.is_empty()) else {
        return empty_note("No inputs", "Load a profile to add inputs.", cx);
    };
    v_flex()
        .gap_1()
        .p_2()
        .children(
            profile
                .sources
                .iter()
                .map(|source| source_row(source, view, cx)),
        )
        .into_any_element()
}

fn source_row(source: &Source, view: &ProducerView, cx: &mut Context<ProducerView>) -> AnyElement {
    let uuid = source.uuid.clone();
    let mute_uuid = source.uuid.clone();
    let muted = view.is_muted(&source.uuid);

    h_flex()
        .id(format!("source-row:{uuid}"))
        .gap_2()
        .items_center()
        .rounded_md()
        .px_1()
        .py_1()
        .child(
            Button::new(format!("source:{uuid}"))
                .when(
                    view.selected_source() == Some(uuid.as_str()),
                    Button::primary,
                )
                .when(view.selected_source() != Some(uuid.as_str()), Button::ghost)
                .flex_1()
                .icon(input_icon(source.input))
                .label(source.name.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_source(uuid.clone());
                    cx.notify();
                })),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(source.kind.label()),
        )
        .child(
            Button::new(format!("mute:{mute_uuid}"))
                .ghost()
                .xsmall()
                .icon(if muted {
                    IconName::VolumeX
                } else {
                    IconName::Volume2
                })
                .tooltip(if muted { "Unmute" } else { "Mute" })
                .label("")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_mute(&mute_uuid);
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// The short form shown on the button; the full label is the tooltip.
fn short_label(preset: studio_engine::AspectRatio) -> &'static str {
    match preset {
        studio_engine::AspectRatio::Landscape => "16:9",
        studio_engine::AspectRatio::Vertical => "9:16",
        studio_engine::AspectRatio::Square => "1:1",
        studio_engine::AspectRatio::Portrait => "4:5",
        studio_engine::AspectRatio::Custom => "Custom",
    }
}

fn input_icon(input: InputKind) -> IconName {
    match input {
        InputKind::Display | InputKind::Window => IconName::Monitor,
        InputKind::Game => IconName::Gamepad2,
        InputKind::Camera => IconName::Video,
        InputKind::Microphone => IconName::Mic,
        InputKind::Media => IconName::FilePlay,
    }
}

fn mixer(
    profile: Option<&Profile>,
    view: &ProducerView,
    cx: &mut Context<ProducerView>,
) -> AnyElement {
    let Some(profile) = profile.filter(|profile| !profile.audio_channels.is_empty()) else {
        return empty_note("No channels", "Load a profile to see the mixer.", cx);
    };
    v_flex()
        .gap_3()
        .p_3()
        .children(profile.audio_channels.iter().map(|channel: &AudioChannel| {
            let sources: Vec<Source> = profile
                .channel_sources(&channel.name)
                .into_iter()
                .cloned()
                .collect();
            v_flex()
                .id(format!("channel:{}", channel.name))
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(channel.name.clone()),
                )
                .children(sources.into_iter().map(|source| {
                    let uuid = source.uuid.clone();
                    let muted = view.is_muted(&source.uuid);
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_sm().flex_1().child(source.name.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{:.0} dB", source.gain_db)),
                        )
                        .child(
                            Button::new(format!("mix-mute:{uuid}"))
                                .ghost()
                                .xsmall()
                                .icon(if muted {
                                    IconName::VolumeX
                                } else {
                                    IconName::Volume2
                                })
                                .label("")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_mute(&uuid);
                                    cx.notify();
                                })),
                        )
                }))
        }))
        .into_any_element()
}

/// What the party card says when the platform has not described the party.
///
/// Split out so the wording is one decision, asserted directly, rather than buried in
/// layout code.
fn party_card_fallback(remembered: Option<&str>) -> (String, String) {
    match remembered {
        Some(id) => (
            format!("Party {id}"),
            "Remembered from your last session. Its roster loads when the platform answers."
                .to_string(),
        ),
        None => (
            "No party selected".to_string(),
            "Finish onboarding to choose one.".to_string(),
        ),
    }
}

/// The party dock's header: what the console actually knows.
///
/// `remembered` is the party id from `console.json`. The summary only exists when the
/// platform answered during onboarding, so after a restart it is `None` even though the
/// console was told which party to publish into. Saying "no party selected" there was a
/// lie: the id is on disk and the publish path will use it.
fn party_card(
    party: Option<&studio_party::PartySummary>,
    remembered: Option<&str>,
    cx: &mut Context<ProducerView>,
) -> AnyElement {
    let theme = cx.theme();
    let Some(party) = party else {
        let (heading, detail) = party_card_fallback(remembered);
        return v_flex()
            .id("party-card")
            .test_support()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .p_3()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(heading),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(detail),
            )
            .into_any_element();
    };

    let pending = party
        .my_inputs
        .iter()
        .filter(|input| !input.is_approved())
        .count();
    let approved: Vec<&str> = party
        .approved_kinds
        .iter()
        .map(|kind| kind.label())
        .collect();

    v_flex()
        .id("party-card")
        .test_support()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .p_3()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(party.title.clone()),
                )
                .child(
                    Badge::new()
                        .icon(if party.is_live() {
                            IconName::CircleCheck
                        } else {
                            IconName::Circle
                        })
                        .child(if party.is_live() { "Live" } else { "Offline" })
                        .color(if party.is_live() {
                            theme.success
                        } else {
                            theme.muted_foreground
                        }),
                ),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(party.role.to_string()),
                )
                .when_some(party.game_name.as_ref(), |this, game| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(game.clone()),
                    )
                }),
        )
        .when(!approved.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Approved: {}", approved.join(", "))),
            )
        })
        .when(pending > 0, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(theme.warning)
                    .child(format!("{pending} awaiting owner approval")),
            )
        })
        .into_any_element()
}

fn input_rail(rows: Vec<InputRow>, cx: &mut Context<ProducerView>) -> AnyElement {
    if rows.is_empty() {
        return empty_note(
            "Nothing declared",
            "Add an input to ask the owner for approval.",
            cx,
        );
    }
    v_flex()
        .id("inputs-list")
        .test_support()
        .gap_1()
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            let badge = match row.consent {
                Some(ConsentState::Approved) => Badge::new().color(cx.theme().success),
                Some(ConsentState::Pending) => Badge::new().color(cx.theme().warning),
                Some(ConsentState::Revoked) => Badge::new().color(cx.theme().danger),
                // Nothing is invented before the room snapshot arrives.
                None => Badge::new(),
            };
            let state_label = row.consent.map_or("Awaiting sync", ConsentState::label);
            h_flex()
                .id(format!("input:{index}"))
                .justify_between()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .child(
                    v_flex()
                        .gap_0()
                        .child(div().text_sm().child(row.kind.label()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(row.source_name.clone()),
                        ),
                )
                .child(badge.child(state_label.to_string()))
        }))
        .into_any_element()
}

fn video_surface(
    id: &'static str,
    title: &str,
    ratio: f32,
    cx: &mut Context<ProducerView>,
) -> AnyElement {
    v_flex()
        .size_full()
        .gap_2()
        .p_3()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(title.to_string()),
        )
        .child(
            // The stage fills the panel; the canvas inside it takes the chosen shape and
            // is centred, so a tall format letterboxes inside a wide panel instead of
            // stretching to fill it. Previously this box was `flex_1` with no shape at
            // all, so the aspect control changed the profile and nothing else.
            div()
                .id(format!("preview-stage:{id}"))
                .test_support()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id(id)
                        .test_support()
                        .aspect_ratio(ratio)
                        .h_full()
                        .max_w(gpui_kit::relative(1.))
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().group_box)
                        .items_center()
                        .justify_center()
                        .p_4()
                        .child(
                            v_flex()
                                .items_center()
                                .gap_2()
                                .child(Icon::new(IconName::VideoOff).large())
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No video"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("The media engine isn't connected yet."),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

fn empty_note(title: &str, detail: &str, cx: &mut Context<ProducerView>) -> AnyElement {
    v_flex()
        .items_center()
        .justify_center()
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
