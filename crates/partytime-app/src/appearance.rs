//! The appearance control: one button offering System, Light and Dark.
//!
//! A single trigger rather than three buttons, because the choice is a setting rather than
//! a frequent command, and because the current value is worth showing without opening
//! anything. It is a real `Button`, so it is reachable and operable from the keyboard and
//! carries an accessible name.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::{App, IntoElement, RenderOnce};

use crate::theme::{self, ThemePreference};

/// The appearance picker.
///
/// Stateless: it reads the preference at render time, so the checked item cannot drift from
/// what is actually in force.
#[derive(IntoElement)]
pub struct AppearanceMenu {
    /// The preference to show as current.
    pub preference: ThemePreference,
}

impl RenderOnce for AppearanceMenu {
    fn render(self, _window: &mut gpui_kit::Window, _cx: &mut App) -> impl IntoElement {
        let current = self.preference;
        Button::new("appearance")
            .ghost()
            .xsmall()
            .icon(current.icon())
            .label(current.label())
            .tooltip("Appearance")
            .dropdown_menu(move |menu, _window, _cx| {
                ThemePreference::ALL.into_iter().fold(menu, |menu, option| {
                    menu.item(
                        PopupMenuItem::new(option.label())
                            .icon(option.icon())
                            .checked(option == current)
                            .on_click(move |_, _, cx| theme::set_preference(option, cx)),
                    )
                })
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preference_has_a_label_and_an_icon() {
        for preference in ThemePreference::ALL {
            assert!(!preference.label().is_empty());
            // Icons are Lucide names; a typo here is a compile error, not a blank glyph.
            let _ = preference.icon();
        }
        assert_eq!(ThemePreference::ALL.len(), 3);
    }

    #[test]
    fn the_menu_lists_system_first_then_light_then_dark() {
        let labels: Vec<&str> = ThemePreference::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(labels, vec!["System", "Light", "Dark"]);
    }
}
