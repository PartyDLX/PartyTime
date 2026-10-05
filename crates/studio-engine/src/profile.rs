//! Profile model shared by the console UI, the on-disk store, and the media engine.
//!
//! The console is configured like OBS: a profile owns scenes, the sources those
//! scenes contain, and the audio channels those sources feed. Every source also
//! declares the OpenParty publish kind it maps to, because that declaration is the
//! consent boundary described in `docs/PLAN.md` §4.
//!
//! Identity is the OBS `uuid`, never a list index: sources and scenes are reordered
//! and removed constantly, and every `ElementId` in the UI is derived from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use studio_party::PublishKind;

// `PublishKind` is the platform's vocabulary and lives in `studio-party`; this crate
// borrows it rather than restating it, so a local declaration and a wire value cannot
// drift apart.

/// The capture device behind a source.
///
/// [`PublishKind`] is what the *party* sees; this is what the *machine* produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputKind {
    /// Whole-screen capture.
    Display,
    /// Single application window.
    Window,
    /// Game capture (graphics API hook).
    Game,
    /// Video camera.
    Camera,
    /// Audio input device.
    Microphone,
    /// A file or image played back into the scene.
    Media,
}

impl InputKind {
    /// Whether this input produces video.
    #[must_use]
    pub const fn is_video(self) -> bool {
        matches!(
            self,
            Self::Display | Self::Window | Self::Game | Self::Camera | Self::Media
        )
    }

    /// Whether this input produces audio.
    #[must_use]
    pub const fn is_audio(self) -> bool {
        matches!(self, Self::Microphone | Self::Media)
    }

    /// The publish kind a freshly imported source of this type declares.
    #[must_use]
    pub const fn default_publish_kind(self) -> PublishKind {
        match self {
            Self::Display | Self::Window | Self::Game => PublishKind::Gameplay,
            Self::Camera => PublishKind::Camera,
            Self::Microphone => PublishKind::Mic,
            Self::Media => PublishKind::GameAudio,
        }
    }
}

/// One input in a profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// OBS `uuid`. Stable identity for the UI; never a list index.
    pub uuid: String,
    /// Display name, unique within the profile.
    pub name: String,
    /// What the machine captures.
    pub input: InputKind,
    /// What the party is asked to approve for this source.
    pub kind: PublishKind,
    /// Whether the source is visible in the scene that contains it.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Whether the source contributes audio.
    #[serde(default)]
    pub muted: bool,
    /// Linear gain in decibels, clamped to `[-60, 20]` like OBS.
    #[serde(default)]
    pub gain_db: f32,
    /// Capture-device clock correction, in milliseconds.
    #[serde(default)]
    pub sync_offset_ms: i32,
}

const fn default_true() -> bool {
    true
}

impl Source {
    /// Creates a source with OBS defaults.
    #[must_use]
    pub fn new(uuid: impl Into<String>, name: impl Into<String>, input: InputKind) -> Self {
        Self {
            uuid: uuid.into(),
            name: name.into(),
            input,
            kind: input.default_publish_kind(),
            visible: true,
            muted: false,
            gain_db: 0.0,
            sync_offset_ms: 0,
        }
    }

    /// Clamps gain into the range OBS accepts.
    pub fn clamp_gain(&mut self) {
        self.gain_db = self.gain_db.clamp(-60.0, 20.0);
    }
}

/// How a scene item is placed on the canvas.
///
/// These are OBS's own fields, so an imported scene lands where its author put it. A
/// transform is what the canvas editor manipulates when a layer is dragged or resized.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transform {
    /// Horizontal offset in canvas pixels, relative to `alignment`.
    pub position_x: f32,
    /// Vertical offset in canvas pixels, relative to `alignment`.
    pub position_y: f32,
    /// OBS alignment bitmask: the anchor point the offsets are measured from.
    pub alignment: u32,
    /// Horizontal scale, 1.0 being native size.
    pub scale_x: f32,
    /// Vertical scale.
    pub scale_y: f32,
    /// Clockwise rotation in degrees.
    pub rotation: f32,
    /// Explicit width in canvas pixels; `None` means "track the source".
    pub width: Option<f32>,
    /// Explicit height; `None` tracks the source.
    pub height: Option<f32>,
    /// Whether the crop below applies.
    pub crop_enabled: bool,
    /// Crop insets, in canvas pixels.
    /// Left crop inset in canvas pixels.
    pub crop_left: u32,
    /// Right crop inset in canvas pixels.
    pub crop_right: u32,
    /// Top crop inset in canvas pixels.
    pub crop_top: u32,
    /// Bottom crop inset in canvas pixels.
    pub crop_bottom: u32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position_x: 0.0,
            position_y: 0.0,
            // OBS's default: centre (5).
            alignment: 5,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            width: None,
            height: None,
            crop_enabled: false,
            crop_left: 0,
            crop_right: 0,
            crop_top: 0,
            crop_bottom: 0,
        }
    }
}

impl Transform {
    /// Whether this item sits exactly where OBS would put it by default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Moves the item by a canvas-space delta, keeping its alignment anchor.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.position_x += dx;
        self.position_y += dy;
    }

    /// Scales about the item's own anchor.
    pub fn scale_by(&mut self, factor: f32) {
        self.scale_x = (self.scale_x * factor).max(0.01);
        self.scale_y = (self.scale_y * factor).max(0.01);
    }

    /// Clamps the crop so it can never exceed the source.
    pub fn clamp_crop(&mut self, source_width: u32, source_height: u32) {
        self.crop_left = self.crop_left.min(source_width);
        self.crop_right = self.crop_right.min(source_width - self.crop_left);
        self.crop_top = self.crop_top.min(source_height);
        self.crop_bottom = self.crop_bottom.min(source_height - self.crop_top);
    }
}

/// A source placed into a scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneItem {
    /// `uuid` of the [`Source`] this item renders.
    pub source_uuid: String,
    /// Per-scene visibility, independent of the source's own flag.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Where the item sits on the canvas.
    #[serde(default)]
    pub transform: Transform,
}

/// A scene: an ordered set of sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Unique within the profile.
    pub name: String,
    /// Render order, back to front.
    #[serde(default)]
    pub items: Vec<SceneItem>,
}

/// An OBS mixing channel and the sources assigned to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioChannel {
    /// Channel name, unique within the profile.
    pub name: String,
    /// `uuid`s of the sources mixed into this channel.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// The shape of the published output.
///
/// Vertical formats are first-class here rather than a width and height to retype: a
/// creator publishing to Instagram or TikTok should pick "9:16" and have the settings,
/// the canvas and the worker agree, not adjust three numbers by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AspectRatio {
    /// 16:9, the usual stream shape.
    #[default]
    Landscape,
    /// 9:16, vertical video.
    Vertical,
    /// 1:1, square.
    Square,
    /// 4:5, the taller portrait shape.
    Portrait,
    /// Whatever the profile already says.
    Custom,
}

impl AspectRatio {
    /// The ratios a creator picks, in the order they appear.
    pub const PRESETS: [Self; 4] = [
        Self::Landscape,
        Self::Vertical,
        Self::Square,
        Self::Portrait,
    ];

    /// The label shown on the control.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Landscape => "16:9 landscape",
            Self::Vertical => "9:16 vertical",
            Self::Square => "1:1 square",
            Self::Portrait => "4:5 portrait",
            Self::Custom => "Custom",
        }
    }

    /// Width over height, or `None` for [`Self::Custom`].
    #[must_use]
    pub const fn ratio(self) -> Option<(u32, u32)> {
        match self {
            Self::Landscape => Some((16, 9)),
            Self::Vertical => Some((9, 16)),
            Self::Square => Some((1, 1)),
            Self::Portrait => Some((4, 5)),
            Self::Custom => None,
        }
    }

    /// New dimensions for this shape, holding the shorter side of the current output
    /// fixed so switching format never quietly drops resolution.
    #[must_use]
    pub fn apply(self, width: u32, height: u32) -> (u32, u32) {
        let Some((rw, rh)) = self.ratio() else {
            return (width, height);
        };
        let base = width.min(height).max(1);
        if rw >= rh {
            (base * rw / rh, base)
        } else {
            (base, base * rh / rw)
        }
    }

    /// The shape these dimensions already are.
    #[must_use]
    pub fn of(width: u32, height: u32) -> Self {
        if width == height && height != 0 {
            return Self::Square;
        }
        Self::PRESETS
            .into_iter()
            .find(|preset| {
                preset
                    .ratio()
                    .is_some_and(|(rw, rh)| rw * height == rh * width)
            })
            .unwrap_or(Self::Custom)
    }
}

/// Encoder settings that the OpenParty path constrains.
///
/// H.264 is fixed because the worker relays RTP verbatim and every browser must
/// decode it; see `docs/PLAN.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSettings {
    /// The chosen shape.
    #[serde(default)]
    pub aspect: AspectRatio,
    /// Encoded width in pixels.
    pub width: u32,
    /// Encoded height in pixels.
    pub height: u32,
    /// Frames per second, numerator of an NTSC-free rational.
    pub fps: u32,
    /// Target video bitrate in kilobits per second.
    pub bitrate_kbps: u32,
    /// Seconds between keyframes.
    pub keyframe_interval_secs: u32,
    /// Opus bitrate in kilobits per second.
    pub audio_bitrate_kbps: u32,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            aspect: AspectRatio::Landscape,
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 6000,
            keyframe_interval_secs: 2,
            audio_bitrate_kbps: 160,
        }
    }
}

/// Publish behaviour that is OpenParty-specific rather than OBS-specific.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishSettings {
    /// Start publishing as soon as a live party is selected.
    #[serde(default)]
    pub auto_start: bool,
    /// Re-mint and re-publish attempts after a transport failure.
    #[serde(default = "default_reconnect_limit")]
    pub reconnect_limit: u32,
    /// Preferred hardware encoder; `None` means "first available, x264 last".
    #[serde(default)]
    pub encoder: Option<String>,
}

const fn default_reconnect_limit() -> u32 {
    3
}

impl Default for PublishSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            reconnect_limit: default_reconnect_limit(),
            encoder: None,
        }
    }
}

/// A complete publishing profile: the unit the console loads, edits, and exports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Profile name, matching the OBS profile directory name.
    pub name: String,
    /// Output and encoder settings.
    #[serde(default)]
    pub output: OutputSettings,
    /// OpenParty publish behaviour.
    #[serde(default)]
    pub publish: PublishSettings,
    /// Inputs, keyed by nothing in particular — order is presentation only.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// Scenes in UI order.
    #[serde(default)]
    pub scenes: Vec<Scene>,
    /// Mixing channels in UI order.
    #[serde(default)]
    pub audio_channels: Vec<AudioChannel>,
}

impl Profile {
    /// Creates an empty profile with the default `Gameplay` and `Mic` sources a
    /// first-time creator needs, so the console is never empty on arrival.
    #[must_use]
    pub fn starter(name: impl Into<String>) -> Self {
        let screen = Source::new("screen", "Game Capture", InputKind::Game);
        let mic = Source::new("mic", "Mic", InputKind::Microphone);
        Self {
            name: name.into(),
            output: OutputSettings::default(),
            publish: PublishSettings::default(),
            sources: vec![screen, mic],
            scenes: vec![Scene {
                name: "Party Scene".into(),
                items: vec![
                    SceneItem {
                        source_uuid: "screen".into(),
                        visible: true,
                        transform: Transform::default(),
                    },
                    SceneItem {
                        source_uuid: "mic".into(),
                        visible: true,
                        transform: Transform::default(),
                    },
                ],
            }],
            audio_channels: vec![
                AudioChannel {
                    name: "Default".into(),
                    sources: vec!["screen".into()],
                },
                AudioChannel {
                    name: "Mic/Aux".into(),
                    sources: vec!["mic".into()],
                },
            ],
        }
    }

    /// Looks up a source by `uuid`.
    #[must_use]
    pub fn source(&self, uuid: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.uuid == uuid)
    }

    /// Looks up a source by `uuid` for mutation.
    pub fn source_mut(&mut self, uuid: &str) -> Option<&mut Source> {
        self.sources.iter_mut().find(|s| s.uuid == uuid)
    }

    /// Looks up a scene by name.
    #[must_use]
    pub fn scene(&self, name: &str) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.name == name)
    }

    /// The publish kinds declared by the sources in `scene`, in `PublishKind` order.
    ///
    /// This is the input to the consent comparison in the preflight sheet: the
    /// program's declared kinds against the kinds the party approved.
    #[must_use]
    pub fn declared_kinds_in(&self, scene_name: &str) -> Vec<PublishKind> {
        let Some(scene) = self.scene(scene_name) else {
            return Vec::new();
        };
        let mut kinds: Vec<PublishKind> = scene
            .items
            .iter()
            .filter(|item| item.visible)
            .filter_map(|item| self.source(&item.source_uuid))
            .map(|source| source.kind)
            .collect();
        kinds.sort_unstable();
        kinds.dedup();
        kinds
    }

    /// Sources assigned to an audio channel, in channel order.
    #[must_use]
    pub fn channel_sources(&self, channel: &str) -> Vec<&Source> {
        let Some(channel) = self.audio_channels.iter().find(|c| c.name == channel) else {
            return Vec::new();
        };
        channel
            .sources
            .iter()
            .filter_map(|uuid| self.source(uuid))
            .collect()
    }

    /// Every source whose publish kind is in `approved`, keyed by uuid.
    ///
    /// The published program is built from this set: anything not returned here is
    /// never rendered into the encoded stream, which is what makes INVARIANT C1
    /// structural rather than a filter that could be misconfigured.
    #[must_use]
    pub fn publishable_sources(
        &self,
        approved: &BTreeMap<PublishKind, bool>,
    ) -> BTreeMap<&str, &Source> {
        self.sources
            .iter()
            .filter(|source| approved.get(&source.kind).copied().unwrap_or(false))
            .map(|source| (source.uuid.as_str(), source))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_profile_is_usable() {
        let profile = Profile::starter("Default");
        assert_eq!(profile.scenes.len(), 1);
        assert_eq!(profile.sources.len(), 2);
        assert!(profile.source("mic").is_some());
    }

    #[test]
    fn declared_kinds_follow_scene_visibility_not_source_visibility() {
        let mut profile = Profile::starter("Default");
        // Hiding the source itself must not change the program's declared kinds:
        // the scene item governs what the program contains.
        profile.source_mut("mic").expect("mic").visible = false;
        assert_eq!(
            profile.declared_kinds_in("Party Scene"),
            vec![PublishKind::Gameplay, PublishKind::Mic]
        );

        profile.scenes[0].items[1].visible = false;
        assert_eq!(
            profile.declared_kinds_in("Party Scene"),
            vec![PublishKind::Gameplay]
        );
    }

    #[test]
    fn unknown_scene_declares_nothing() {
        let profile = Profile::starter("Default");
        assert!(profile.declared_kinds_in("Nope").is_empty());
    }

    #[test]
    fn publishable_sources_excludes_unapproved_kinds() {
        let profile = Profile::starter("Default");
        let mut approved = BTreeMap::new();
        approved.insert(PublishKind::Gameplay, true);
        approved.insert(PublishKind::Mic, false);

        let publishable = profile.publishable_sources(&approved);
        assert_eq!(publishable.len(), 1);
        assert!(publishable.contains_key("screen"));
        assert!(
            !publishable.contains_key("mic"),
            "unapproved audio must not be publishable"
        );
    }

    #[test]
    fn missing_approval_is_treated_as_not_approved() {
        let profile = Profile::starter("Default");
        assert!(profile.publishable_sources(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn gain_is_clamped_to_the_obs_range() {
        let mut source = Source::new("a", "A", InputKind::Microphone);
        source.gain_db = 90.0;
        source.clamp_gain();
        assert_eq!(source.gain_db, 20.0);
        source.gain_db = -200.0;
        source.clamp_gain();
        assert_eq!(source.gain_db, -60.0);
    }

    #[test]
    fn every_preset_resolves_to_its_own_ratio() {
        for preset in AspectRatio::PRESETS {
            let (rw, rh) = preset.ratio().expect("a preset has a ratio");
            let (w, h) = preset.apply(1920, 1080);
            assert_eq!(
                w * rh,
                h * rw,
                "{} did not resolve to its ratio",
                preset.label()
            );
        }
    }

    #[test]
    fn going_vertical_keeps_the_pixels_and_flips_the_shape() {
        let (w, h) = AspectRatio::Vertical.apply(1920, 1080);
        assert!(h > w, "9:16 must be taller than wide, got {w}x{h}");
        assert_eq!(
            w * h,
            1920 * 1080,
            "switching format must not drop resolution"
        );
        assert_eq!(w, 1080, "the short side stays at the old height");
    }

    #[test]
    fn the_shape_is_recognised_from_the_dimensions() {
        assert_eq!(AspectRatio::of(1920, 1080), AspectRatio::Landscape);
        assert_eq!(AspectRatio::of(1080, 1920), AspectRatio::Vertical);
        assert_eq!(AspectRatio::of(1080, 1350), AspectRatio::Portrait);
        assert_eq!(AspectRatio::of(1080, 1080), AspectRatio::Square);
        assert_eq!(AspectRatio::of(1333, 999), AspectRatio::Custom);
    }

    #[test]
    fn custom_leaves_the_dimensions_alone() {
        assert_eq!(AspectRatio::Custom.apply(1234, 567), (1234, 567));
    }

    #[test]
    fn publish_kind_video_and_audio_are_disjoint() {
        for kind in PublishKind::ALL {
            assert!(!(kind.is_video() && kind.is_audio()), "{kind:?} is both");
        }
        assert_eq!(PublishKind::ALL.iter().filter(|k| k.is_video()).count(), 2);
        assert_eq!(PublishKind::ALL.iter().filter(|k| k.is_audio()).count(), 3);
    }

    #[test]
    fn channel_sources_resolve_uuids_and_skip_dangling_ones() {
        let mut profile = Profile::starter("Default");
        profile.audio_channels[0].sources.push("ghost".into());
        let sources = profile.channel_sources("Default");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].uuid, "screen");
        assert!(profile.channel_sources("Nope").is_empty());
    }
}
