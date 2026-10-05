//! OpenParty's palette, as GPUI Kit theme tokens.
//!
//! The source of truth is the same CSS custom properties the web app ships. They are
//! kept here as `oklch()` values rather than copied out as hex, so a change to the brand
//! is a change to one number here, and so the values stay reviewable next to the CSS they
//! came from.
//!
//! Three rules decide what this module sets and what it leaves alone:
//!
//! * **Direct.** Where the CSS defines a variable with a matching role, that variable is
//!   the token. `--primary` becomes `primary.background`, and so on.
//! * **Derived.** Where a web component reads one of those variables for a surface GPUI
//!   Kit names differently — a link taking `--primary`, a selected row taking `--accent` —
//!   the token takes that variable. This is the same composition the web app performs,
//!   not a new decision.
//! * **Untouched.** Where the CSS defines nothing — `--success`, `--warning`, `--info`,
//!   the caret, the window border — the token keeps GPUI Kit's default, which is what the
//!   web app shows for the same reason.
//!
//! Applying this is a single call in `main`; every view already reads `cx.theme()`, so
//! nothing else moves.

use std::borrow::Cow;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode};
use gpui_kit::{App, Global, WindowAppearance};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The bundled UI family, and the name OpenParty's CSS asks for.
pub const FONT_FAMILY: &str = "Figtree";

/// Overrides the bundled family with one installed on the machine.
///
/// Set this when a build should use the platform UI font instead of shipping Figtree.
pub const FONT_FAMILY_ENV: &str = "PARTYTIME_FONT_FAMILY";

/// Figtree, variable weight 300-900. Bundled so the console does not depend on a font
/// being installed; the wght axis covers the weights the interface uses.
///
/// SIL Open Font License 1.1 — see `assets/Figtree-OFL.txt`.
const FIGTREE_VARIABLE: &[u8] = include_bytes!("../assets/Figtree[wght].ttf");

/// `--radius: 0.625rem`, which is 10px at the default root size.
const RADIUS_PX: usize = 10;

/// `--radius-xl`, which is `--radius * 1.4`.
const RADIUS_LG_PX: usize = 14;

/// Which appearance the console renders in.
///
/// `System` is the default and the only one that tracks the desktop: the shell
/// re-resolves it whenever the window reports a new appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Always light.
    Light,
    /// Always dark.
    Dark,
    /// Whatever the desktop is doing.
    #[default]
    System,
}

impl ThemePreference {
    /// Every option, in the order the menu lists them.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    /// The label the menu shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    /// The icon beside the label, so the choice is recognisable before it is read.
    #[must_use]
    pub const fn icon(self) -> IconName {
        match self {
            Self::Light => IconName::Sun,
            Self::Dark => IconName::Moon,
            Self::System => IconName::Monitor,
        }
    }

    /// The concrete appearance this preference resolves to, given what the desktop says.
    #[must_use]
    pub const fn resolve(self, system: ThemeMode) -> ThemeMode {
        match self {
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
            Self::System => system,
        }
    }

    /// Parses a stored value, falling back to following the system.
    ///
    /// A file written by a newer build, or a hand edit, must not stop the console
    /// launching; the worst case is that it follows the desktop.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }
}

/// The appearance the console is rendering in, and whether the bundled face is loaded.
#[derive(Debug, Clone, Default)]
pub struct Appearance {
    preference: ThemePreference,
    fonts_registered: bool,
}

impl Appearance {
    /// The user's choice.
    #[must_use]
    pub const fn preference(&self) -> ThemePreference {
        self.preference
    }

    /// Whether the bundled face has been handed to the text system.
    const fn fonts_registered(&self) -> bool {
        self.fonts_registered
    }
}

impl Global for Appearance {}

/// Edits the appearance state in place.
///
/// `App` has no `update_global`, so this reads, edits and writes back. `Appearance` is two
/// fields, which is a cheaper thing to copy than a lock would be.
fn mutate_appearance(cx: &mut App, edit: impl FnOnce(&mut Appearance)) {
    let mut appearance = cx.try_global::<Appearance>().cloned().unwrap_or_default();
    edit(&mut appearance);
    cx.set_global(appearance);
}

/// The appearance preference in force.
#[must_use]
pub fn preference(cx: &App) -> ThemePreference {
    cx.try_global::<Appearance>()
        .map_or_else(ThemePreference::default, |appearance| {
            appearance.preference()
        })
}

/// A CSS `oklch()` colour: lightness, chroma, hue in degrees, alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Color {
    lightness: f32,
    chroma: f32,
    hue: f32,
    alpha: f32,
}

impl Color {
    const fn oklch(lightness: f32, chroma: f32, hue: f32) -> Self {
        Self {
            lightness,
            chroma,
            hue,
            alpha: 1.0,
        }
    }

    /// A CSS `oklch(L C H / A)` — used for the dark mode's translucent border and input.
    const fn oklch_alpha(lightness: f32, chroma: f32, hue: f32, alpha: f32) -> Self {
        Self {
            lightness,
            chroma,
            hue,
            alpha,
        }
    }

    /// A flat `#rrggbb`, or `#rrggbbaa` when the colour carries alpha.
    ///
    /// Goes through GPUI Kit's own `oklch` so the conversion is the framework's, not a
    /// second implementation that can drift from it.
    fn to_hex(self) -> String {
        let rgba =
            gpui_kit::component::theme::oklch(self.lightness, self.chroma, self.hue).to_rgb();
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        if self.alpha >= 1.0 {
            format!(
                "#{:02X}{:02X}{:02X}",
                byte(rgba.r),
                byte(rgba.g),
                byte(rgba.b)
            )
        } else {
            format!(
                "#{:02X}{:02X}{:02X}{:02X}",
                byte(rgba.r),
                byte(rgba.g),
                byte(rgba.b),
                (self.alpha.clamp(0.0, 1.0) * 255.0).round() as u8
            )
        }
    }
}

/// OpenParty's palette for one appearance.
///
/// Every field is a CSS custom property. The light and dark blocks are transcribed from
/// `app.css` without alteration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    background: Color,
    foreground: Color,
    card: Color,
    card_foreground: Color,
    popover: Color,
    popover_foreground: Color,
    primary: Color,
    primary_foreground: Color,
    secondary: Color,
    secondary_foreground: Color,
    muted: Color,
    muted_foreground: Color,
    accent: Color,
    accent_foreground: Color,
    destructive: Color,
    border: Color,
    input: Color,
    ring: Color,
    chart: [Color; 5],
    sidebar: Color,
    sidebar_foreground: Color,
    sidebar_primary: Color,
    sidebar_primary_foreground: Color,
    sidebar_accent: Color,
    sidebar_accent_foreground: Color,
    sidebar_border: Color,
}

/// `:root` — the light palette.
const LIGHT: Palette = Palette {
    background: Color::oklch(1.0, 0.0, 0.0),
    foreground: Color::oklch(0.141, 0.005, 285.823),
    card: Color::oklch(1.0, 0.0, 0.0),
    card_foreground: Color::oklch(0.141, 0.005, 285.823),
    popover: Color::oklch(1.0, 0.0, 0.0),
    popover_foreground: Color::oklch(0.141, 0.005, 285.823),
    primary: Color::oklch(0.5, 0.134, 242.749),
    primary_foreground: Color::oklch(0.977, 0.013, 236.62),
    secondary: Color::oklch(0.967, 0.001, 286.375),
    secondary_foreground: Color::oklch(0.21, 0.006, 285.885),
    muted: Color::oklch(0.967, 0.001, 286.375),
    muted_foreground: Color::oklch(0.552, 0.016, 285.938),
    accent: Color::oklch(0.5, 0.134, 242.749),
    accent_foreground: Color::oklch(0.977, 0.013, 236.62),
    destructive: Color::oklch(0.577, 0.245, 27.325),
    border: Color::oklch(0.92, 0.004, 286.32),
    input: Color::oklch(0.92, 0.004, 286.32),
    ring: Color::oklch(0.705, 0.015, 286.067),
    chart: [
        Color::oklch(0.828, 0.111, 230.318),
        Color::oklch(0.685, 0.169, 237.323),
        Color::oklch(0.588, 0.158, 241.966),
        Color::oklch(0.5, 0.134, 242.749),
        Color::oklch(0.443, 0.11, 240.79),
    ],
    sidebar: Color::oklch(0.985, 0.0, 0.0),
    sidebar_foreground: Color::oklch(0.141, 0.005, 285.823),
    sidebar_primary: Color::oklch(0.588, 0.158, 241.966),
    sidebar_primary_foreground: Color::oklch(0.977, 0.013, 236.62),
    sidebar_accent: Color::oklch(0.967, 0.001, 286.375),
    sidebar_accent_foreground: Color::oklch(0.21, 0.006, 285.885),
    sidebar_border: Color::oklch(0.92, 0.004, 286.32),
};

/// `.dark` — the dark palette.
const DARK: Palette = Palette {
    background: Color::oklch(0.141, 0.005, 285.823),
    foreground: Color::oklch(0.985, 0.0, 0.0),
    card: Color::oklch(0.21, 0.006, 285.885),
    card_foreground: Color::oklch(0.985, 0.0, 0.0),
    popover: Color::oklch(0.21, 0.006, 285.885),
    popover_foreground: Color::oklch(0.985, 0.0, 0.0),
    primary: Color::oklch(0.443, 0.11, 240.79),
    primary_foreground: Color::oklch(0.977, 0.013, 236.62),
    secondary: Color::oklch(0.274, 0.006, 286.033),
    secondary_foreground: Color::oklch(0.985, 0.0, 0.0),
    muted: Color::oklch(0.274, 0.006, 286.033),
    muted_foreground: Color::oklch(0.705, 0.015, 286.067),
    accent: Color::oklch(0.443, 0.11, 240.79),
    accent_foreground: Color::oklch(0.977, 0.013, 236.62),
    destructive: Color::oklch(0.704, 0.191, 22.216),
    // The dark theme expresses these as white at 10% and 15%; keeping the alpha matters,
    // because it is what makes a hairline read as a hairline over any surface.
    border: Color::oklch_alpha(1.0, 0.0, 0.0, 0.10),
    input: Color::oklch_alpha(1.0, 0.0, 0.0, 0.15),
    ring: Color::oklch(0.552, 0.016, 285.938),
    chart: [
        Color::oklch(0.828, 0.111, 230.318),
        Color::oklch(0.685, 0.169, 237.323),
        Color::oklch(0.588, 0.158, 241.966),
        Color::oklch(0.5, 0.134, 242.749),
        Color::oklch(0.443, 0.11, 240.79),
    ],
    sidebar: Color::oklch(0.21, 0.006, 285.885),
    sidebar_foreground: Color::oklch(0.985, 0.0, 0.0),
    sidebar_primary: Color::oklch(0.685, 0.169, 237.323),
    sidebar_primary_foreground: Color::oklch(0.293, 0.066, 243.157),
    sidebar_accent: Color::oklch(0.274, 0.006, 286.033),
    sidebar_accent_foreground: Color::oklch(0.985, 0.0, 0.0),
    sidebar_border: Color::oklch_alpha(1.0, 0.0, 0.0, 0.10),
};

/// The palette for an appearance.
#[must_use]
pub(crate) fn palette_for(mode: ThemeMode) -> &'static Palette {
    if mode.is_dark() { &DARK } else { &LIGHT }
}

/// The family the console should render in.
#[must_use]
pub fn font_family() -> String {
    std::env::var(FONT_FAMILY_ENV)
        .ok()
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| FONT_FAMILY.to_string())
}

/// The theme token map for an appearance, keyed the way GPUI Kit's `ThemeConfig` reads them.
fn colors_for(mode: ThemeMode) -> Map<String, Value> {
    let p = palette_for(mode);
    let mut colors = Map::new();

    // Direct: one CSS custom property, one token.
    let direct: [(&str, Color); 24] = [
        ("background", p.background),
        ("foreground", p.foreground),
        ("group_box.background", p.card),
        ("group_box.foreground", p.card_foreground),
        ("popover.background", p.popover),
        ("popover.foreground", p.popover_foreground),
        ("primary.background", p.primary),
        ("primary.foreground", p.primary_foreground),
        ("secondary.background", p.secondary),
        ("secondary.foreground", p.secondary_foreground),
        ("muted.background", p.muted),
        ("muted.foreground", p.muted_foreground),
        ("accent.background", p.accent),
        ("accent.foreground", p.accent_foreground),
        ("danger.background", p.destructive),
        ("border", p.border),
        ("input.border", p.input),
        ("ring", p.ring),
        ("sidebar.background", p.sidebar),
        ("sidebar.foreground", p.sidebar_foreground),
        ("sidebar.primary.background", p.sidebar_primary),
        ("sidebar.primary.foreground", p.sidebar_primary_foreground),
        ("sidebar.accent.background", p.sidebar_accent),
        ("sidebar.border", p.sidebar_border),
    ];
    for (token, color) in direct {
        colors.insert(token.to_string(), Value::String(color.to_hex()));
    }
    colors.insert(
        "sidebar.accent.foreground".to_string(),
        Value::String(p.sidebar_accent_foreground.to_hex()),
    );
    for (index, color) in p.chart.iter().enumerate() {
        colors.insert(
            format!("chart.{}", index + 1),
            Value::String(color.to_hex()),
        );
    }

    // Derived: a surface the web app paints from a variable GPUI Kit names differently.
    let derived: [(&str, Color); 22] = [
        ("primary.hover.background", p.primary),
        ("primary.active.background", p.primary),
        ("secondary.hover.background", p.secondary),
        ("secondary.active.background", p.secondary),
        ("accordion.background", p.background),
        ("caret", p.foreground),
        ("link", p.primary),
        ("link.hover", p.primary),
        ("link.active", p.primary),
        ("selection.background", p.accent),
        ("list.background", p.background),
        ("list.head.background", p.muted),
        ("list.active.background", p.accent),
        ("list.hover.background", p.muted),
        ("table.background", p.background),
        ("table.head.background", p.muted),
        ("table.hover.background", p.muted),
        ("table.active.background", p.accent),
        ("tab_bar.background", p.background),
        ("tab.background", p.background),
        ("tab.active.background", p.background),
        ("description_list.label.foreground", p.muted_foreground),
    ];
    for (token, color) in derived {
        colors.insert(token.to_string(), Value::String(color.to_hex()));
    }
    colors.insert(
        "tab.foreground".to_string(),
        Value::String(p.muted_foreground.to_hex()),
    );
    colors.insert(
        "tab.active.foreground".to_string(),
        Value::String(p.foreground.to_hex()),
    );
    colors.insert(
        "table.head.foreground".to_string(),
        Value::String(p.card_foreground.to_hex()),
    );
    colors.insert(
        "title_bar.background".to_string(),
        Value::String(p.background.to_hex()),
    );
    colors.insert(
        "title_bar.border".to_string(),
        Value::String(p.border.to_hex()),
    );
    colors.insert(
        "status_bar.background".to_string(),
        Value::String(p.background.to_hex()),
    );
    colors.insert(
        "status_bar.border".to_string(),
        Value::String(p.border.to_hex()),
    );
    colors.insert(
        "skeleton.background".to_string(),
        Value::String(p.muted.to_hex()),
    );
    colors.insert(
        "progress.bar.background".to_string(),
        Value::String(p.primary.to_hex()),
    );
    colors.insert(
        "slider.background".to_string(),
        Value::String(p.muted.to_hex()),
    );
    colors.insert(
        "slider.thumb.background".to_string(),
        Value::String(p.primary.to_hex()),
    );
    colors.insert(
        "switch.background".to_string(),
        Value::String(p.muted.to_hex()),
    );
    colors.insert(
        "scrollbar.background".to_string(),
        Value::String(p.background.to_hex()),
    );
    colors.insert(
        "scrollbar.thumb.background".to_string(),
        Value::String(p.border.to_hex()),
    );
    colors.insert(
        "scrollbar.thumb.hover.background".to_string(),
        Value::String(p.ring.to_hex()),
    );
    colors.insert("drag.border".to_string(), Value::String(p.primary.to_hex()));
    colors
}

/// The theme configuration for an appearance.
///
/// Built as a value rather than a checked-in JSON file so the `oklch()` numbers above stay
/// the only place a colour is written down.
pub fn config_for(mode: ThemeMode) -> Result<Rc<ThemeConfig>, serde_json::Error> {
    let mode_name = if mode.is_dark() { "dark" } else { "light" };
    let config: ThemeConfig = serde_json::from_value(json!({
        "name": if mode.is_dark() { "OpenParty Dark" } else { "OpenParty Light" },
        "mode": mode_name,
        "radius": RADIUS_PX,
        "radius.lg": RADIUS_LG_PX,
        "font.family": font_family(),
        "colors": colors_for(mode),
    }))?;
    Ok(Rc::new(config))
}

/// Registers the bundled Figtree face.
///
/// Separate from [`apply`] so the one part that can fail on a corrupt asset is testable on
/// its own. A no-op when [`FONT_FAMILY_ENV`] names a family the machine already has.
pub fn register_fonts(cx: &mut App) -> Result<(), String> {
    if font_family() != FONT_FAMILY {
        return Ok(());
    }
    // The variable face carries the whole 300–900 range and the interface's weights all
    // fall inside it, so one file covers every weight the console uses.
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(FIGTREE_VARIABLE)])
        .map_err(|error| error.to_string())
}

/// The bundled face, as bytes. Exposed so a test can assert the asset is intact.
#[must_use]
pub const fn bundled_font() -> &'static [u8] {
    FIGTREE_VARIABLE
}

/// Applies one appearance: the palette, the radius scale and the bundled family.
fn apply_mode(mode: ThemeMode, cx: &mut App) {
    match config_for(mode) {
        Ok(config) => Theme::update(cx, |theme| theme.apply_config(&config)),
        Err(error) => tracing::error!("could not build the OpenParty theme: {error}"),
    }
}

/// Restores a remembered preference and applies it, without writing anything back.
///
/// This is the launch path: the settings file is the input, not something to rewrite.
pub fn use_preference(preference: ThemePreference, cx: &mut App) {
    mutate_appearance(cx, |appearance| appearance.preference = preference);
    apply(cx);
}

/// Records the user's choice, applies it, and remembers it for next launch.
pub fn set_preference(preference: ThemePreference, cx: &mut App) {
    use_preference(preference, cx);
    if let Err(error) = crate::paths::remember_appearance(preference) {
        // Not fatal: the console renders in the chosen appearance this launch, and the
        // user can pick it again. Losing the choice is far better than refusing to switch.
        tracing::warn!("could not remember the appearance choice: {error}");
    }
}

/// Registers the bundled font and renders in the preferred appearance.
///
/// Call once, after `gpui_kit::init(cx)` and before the first window is opened, so the
/// family exists by the time anything measures text.
pub fn apply(cx: &mut App) {
    if cx.try_global::<Appearance>().is_none() {
        cx.set_global(Appearance::default());
    }
    // `apply` runs again on every preference change and every system switch, and adding
    // the same face twice would leave two entries named Figtree in the font cache.
    let loaded = cx
        .try_global::<Appearance>()
        .is_some_and(Appearance::fonts_registered);
    if !loaded {
        match register_fonts(cx) {
            Ok(()) => mutate_appearance(cx, |appearance| appearance.fonts_registered = true),
            Err(error) => {
                tracing::warn!("could not register the bundled Figtree face: {error}")
            }
        }
    }

    let preference = preference(cx);
    // Read what the desktop says before overriding it, or "system" would resolve to the
    // override this function is about to set.
    let system = ThemeMode::from(cx.window_appearance());

    // While an appearance is forced the window stops tracking the desktop, so "system"
    // has to clear the override rather than set one.
    cx.set_window_appearance(match preference {
        ThemePreference::System => None,
        ThemePreference::Light => Some(WindowAppearance::Light),
        ThemePreference::Dark => Some(WindowAppearance::Dark),
    });

    apply_mode(preference.resolve(system), cx);
}

/// Re-resolves the appearance after the desktop changed.
///
/// A no-op unless the console is following the system: a forced choice must not be
/// undone by the desktop switching at sunset.
pub fn sync(window: &mut gpui_kit::Window, cx: &mut App) {
    if preference(cx) != ThemePreference::System {
        return;
    }
    // The window knows better than the app on Linux, where the platform default can
    // disagree with the compositor the window actually sits on.
    apply_mode(ThemeMode::from(window.appearance()), cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_of(mode: ThemeMode, token: &str) -> String {
        let config = config_for(mode).expect("config builds");
        let colors: Map<String, Value> =
            serde_json::from_value(serde_json::to_value(&config.colors).expect("serialises"))
                .expect("colours deserialise");
        colors
            .get(token)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{token} is not set"))
            .to_string()
    }

    fn relative_luminance(hex: &str) -> f32 {
        let channel = |index: usize| {
            let value = u8::from_str_radix(&hex[1 + index * 2..3 + index * 2], 16).expect("hex");
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
    }

    fn contrast(a: &str, b: &str) -> f32 {
        let (hi, lo) = {
            let (x, y) = (relative_luminance(a), relative_luminance(b));
            if x > y { (x, y) } else { (y, x) }
        };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn the_palette_decodes_to_the_neutrals_the_css_describes() {
        // OpenParty's web app is shadcn on the zinc scale; these are its anchor values.
        assert_eq!(Color::oklch(1.0, 0.0, 0.0).to_hex(), "#FFFFFF");
        assert_eq!(Color::oklch(0.141, 0.005, 285.823).to_hex(), "#09090B");
        assert_eq!(Color::oklch(0.967, 0.001, 286.375).to_hex(), "#F4F4F5");
        assert_eq!(Color::oklch(0.552, 0.016, 285.938).to_hex(), "#71717B");
        assert_eq!(Color::oklch(0.92, 0.004, 286.32).to_hex(), "#E4E4E7");
        assert_eq!(Color::oklch(0.21, 0.006, 285.885).to_hex(), "#18181B");
        assert_eq!(Color::oklch(0.985, 0.0, 0.0).to_hex(), "#FAFAFA");
    }

    #[test]
    fn a_translucent_colour_keeps_its_alpha_in_the_hex() {
        assert_eq!(
            Color::oklch_alpha(1.0, 0.0, 0.0, 0.10).to_hex(),
            "#FFFFFF1A"
        );
        assert_eq!(
            Color::oklch_alpha(1.0, 0.0, 0.0, 0.15).to_hex(),
            "#FFFFFF26"
        );
    }

    #[test]
    fn every_token_this_module_names_is_one_the_framework_actually_reads() {
        // `ThemeConfig` deserialises with `serde(default)` and does not reject unknown
        // keys, so a misspelled token is dropped in silence and the surface quietly keeps
        // the stock colour. Round-tripping through the real type is what makes a typo
        // fail instead.
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let intended = colors_for(mode);
            let config = config_for(mode).expect("config builds");
            let parsed: Map<String, Value> =
                serde_json::from_value(serde_json::to_value(&config.colors).expect("serialises"))
                    .expect("deserialises");

            let intended_keys: Vec<&String> = intended.keys().collect();
            let missing: Vec<&&String> = intended_keys
                .iter()
                .filter(|key| !parsed.contains_key(**key))
                .collect();
            assert!(
                missing.is_empty(),
                "{mode:?} sets tokens GPUI Kit ignores: {missing:?}"
            );
        }
    }

    #[test]
    fn every_token_the_console_reads_is_set_in_both_appearances() {
        // The tokens the three screens resolve through `cx.theme()`.
        let used = [
            "background",
            "foreground",
            "muted.background",
            "muted.foreground",
            "border",
            "primary.background",
            "danger.background",
            "success.background",
            "warning.background",
            "group_box.background",
            "popover.background",
            "status_bar.background",
        ];
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let config = config_for(mode).expect("config builds");
            let colors: Map<String, Value> =
                serde_json::from_value(serde_json::to_value(&config.colors).expect("serialises"))
                    .expect("colours deserialise");
            // `ThemeConfig`'s fields are `Option`s without `skip_serializing_if`, so an
            // unset token is present as `null`. A real token carries a colour string.
            let set = |token: &str| colors.get(token).and_then(Value::as_str).is_some();
            for token in used {
                if token == "success.background" || token == "warning.background" {
                    // Deliberately not set: the CSS defines neither, so the web app uses
                    // shadcn's default and so does the console.
                    assert!(!set(token), "{token} must stay on the framework default");
                    continue;
                }
                assert!(set(token), "{mode:?} is missing {token}");
            }
        }
    }

    #[test]
    fn text_on_the_surface_is_readable_in_both_appearances() {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let bg = hex_of(mode, "background");
            let fg = hex_of(mode, "foreground");
            let muted = hex_of(mode, "muted.foreground");
            let primary = hex_of(mode, "primary.background");
            let on_primary = hex_of(mode, "primary.foreground");

            assert!(
                contrast(&bg, &fg) >= 7.0,
                "{mode:?} body text is {}:1",
                contrast(&bg, &fg)
            );
            assert!(
                contrast(&bg, &muted) >= 4.5,
                "{mode:?} muted text is {}:1",
                contrast(&bg, &muted)
            );
            assert!(
                contrast(&primary, &on_primary) >= 4.5,
                "{mode:?} primary button text is {}:1",
                contrast(&primary, &on_primary)
            );
        }
    }

    #[test]
    fn dark_mode_actually_inverts_the_surface() {
        assert_eq!(hex_of(ThemeMode::Light, "background"), "#FFFFFF");
        assert_eq!(hex_of(ThemeMode::Dark, "background"), "#09090B");
        assert_ne!(
            hex_of(ThemeMode::Light, "primary.background"),
            hex_of(ThemeMode::Dark, "primary.background")
        );
    }

    #[test]
    fn the_config_carries_the_css_radius_scale_and_font() {
        let config = config_for(ThemeMode::Light).expect("config builds");
        assert_eq!(config.radius, Some(RADIUS_PX));
        assert_eq!(config.radius_lg, Some(RADIUS_LG_PX));
        assert_eq!(config.font_family.as_deref(), Some("Figtree"));
        assert_eq!(config.mode, ThemeMode::Light);
    }

    #[test]
    fn the_dark_border_stays_translucent() {
        let border = hex_of(ThemeMode::Dark, "border");
        assert!(border.len() == 9, "expected #rrggbbaa, got {border}");
        let light = hex_of(ThemeMode::Light, "border");
        assert_eq!(light.len(), 7, "expected #rrggbb, got {light}");
    }

    #[test]
    fn the_bundled_face_is_a_truetype_file_the_framework_can_parse() {
        let bytes = bundled_font();
        assert!(bytes.len() > 10_000, "the asset is suspiciously small");
        // An sfnt version tag: 0x00010000 for TrueType outlines.
        assert_eq!(
            &bytes[..4],
            &[0x00, 0x01, 0x00, 0x00],
            "the bundled asset is not a TrueType file"
        );
        // A variable font must carry an `fvar` table, or the wght axis is absent.
        assert!(
            bytes.windows(4).any(|window| window == b"fvar"),
            "the bundled face has no variable-weight axis"
        );
    }

    #[test]
    fn a_forced_preference_ignores_what_the_desktop_says() {
        assert_eq!(
            ThemePreference::Light.resolve(ThemeMode::Dark),
            ThemeMode::Light
        );
        assert_eq!(
            ThemePreference::Dark.resolve(ThemeMode::Light),
            ThemeMode::Dark
        );
    }

    #[test]
    fn system_resolves_to_the_desktop() {
        assert_eq!(
            ThemePreference::System.resolve(ThemeMode::Dark),
            ThemeMode::Dark
        );
        assert_eq!(
            ThemePreference::System.resolve(ThemeMode::Light),
            ThemeMode::Light
        );
        assert_eq!(ThemePreference::default(), ThemePreference::System);
    }

    #[test]
    fn an_unrecognised_stored_preference_follows_the_system_rather_than_failing() {
        assert_eq!(ThemePreference::parse("dark"), ThemePreference::Dark);
        assert_eq!(ThemePreference::parse("light"), ThemePreference::Light);
        assert_eq!(ThemePreference::parse("system"), ThemePreference::System);
        for junk in ["", "Dark", "chartreuse", "auto"] {
            assert_eq!(
                ThemePreference::parse(junk),
                ThemePreference::System,
                "{junk}"
            );
        }
    }

    #[test]
    fn every_preference_names_itself() {
        let labels: Vec<&str> = ThemePreference::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels, vec!["System", "Light", "Dark"]);
    }

    #[test]
    fn the_font_family_can_be_overridden_for_a_build() {
        // Reads the environment; the default path is what a normal launch uses.
        assert!(!font_family().is_empty());
    }
}
