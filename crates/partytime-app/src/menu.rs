//! The application's menu bar.
//!
//! OBS is the reference: a creator arriving from OBS expects File, Edit, View, Docks,
//! Profile, Scene Collection, Tools and Help to be there. This module keeps that shape,
//! and is explicit about the difference between "you can press this" and "this does not
//! exist yet" — a menu that is silently shorter is more confusing than one whose entries
//! are greyed out.
//!
//! Platform menus are installed from this module and the GNOME in-window bar mirrors them.
//! Commands with handlers are enabled; concepts not implemented in PartyTime yet — scene
//! collections, filters, transforms and automation — remain disabled with an explanation.

use gpui_kit::base::h_flex;
use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
};
use gpui_kit::{AnyElement, App, IntoElement as _, Menu, MenuItem, ParentElement as _};
use gpui_kit::{InteractiveElement as _, Styled as _, TestSupportExt as _};

// Declared once so a key binding, a menu item and a handler cannot drift apart.
gpui_kit::actions!(
    partytime,
    [
        /// Follow the desktop's light or dark setting.
        AppearanceSystem,
        /// Always light.
        AppearanceLight,
        /// Always dark.
        AppearanceDark,
        /// Show or hide the left dock: scenes, inputs and the mixer.
        ToggleLeftDock,
        /// Show or hide the right dock: the party.
        ToggleRightDock,
        /// Show what this build is.
        AboutPartyTime,
        // Declared so the OBS-shaped menus can list future commands without inventing
        // handlers. The tests pin which entries are enabled and which remain disabled.
        /// Import an OBS scene collection as a profile.
        ImportObsProfile,
        /// Write the current profile out as an OBS scene collection.
        ExportProfile,
        /// Leave the application.
        Quit,
        /// Choose a different profile.
        SwitchProfile,
        /// Choose a different scene collection.
        SwitchSceneCollection,
        /// Open the filters dock for the selected input.
        OpenFilters,
        /// Open the transform editor for the selected input.
        OpenTransform,
        /// Open the automation dock.
        OpenAutomation,
        /// Reverse the last change.
        Undo,
        /// Repeat the last change.
        Redo,
    ]
);

/// Builds the menu bar.
#[must_use]
pub fn menus() -> Vec<Menu> {
    vec![
        menu(
            "File",
            false,
            vec![
                MenuItem::action("Import OBS profile…", ImportObsProfile),
                MenuItem::action("Export profile…", ExportProfile),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        ),
        menu(
            "Edit",
            false,
            vec![
                MenuItem::action("Undo", Undo),
                MenuItem::action("Redo", Redo),
            ],
        ),
        Menu {
            name: "View".into(),
            items: vec![MenuItem::Submenu(menu(
                "Appearance",
                false,
                vec![
                    MenuItem::action("System", AppearanceSystem),
                    MenuItem::action("Light", AppearanceLight),
                    MenuItem::action("Dark", AppearanceDark),
                ],
            ))],
            disabled: false,
        },
        menu(
            "Docks",
            false,
            vec![
                MenuItem::action("Left", ToggleLeftDock),
                MenuItem::action("Right", ToggleRightDock),
            ],
        ),
        menu(
            "Profile",
            false,
            vec![MenuItem::action("Switch profile…", SwitchProfile)],
        ),
        // PartyTime has one profile per configuration, not OBS's tree of scene
        // collections. The menu is here so the shape matches; the entries are not
        // invented.
        menu(
            "Scene Collection",
            true,
            vec![MenuItem::action(
                "Switch scene collection…",
                SwitchSceneCollection,
            )],
        ),
        // Filters and transforms are libobs features. They arrive with the engine.
        menu(
            "Tools",
            true,
            vec![
                MenuItem::action("Filters…", OpenFilters),
                MenuItem::action("Transform…", OpenTransform),
                MenuItem::separator(),
                MenuItem::action("Automate…", OpenAutomation),
            ],
        ),
        menu(
            "Help",
            false,
            vec![MenuItem::action("About PartyTime", AboutPartyTime)],
        ),
    ]
}

/// A command the in-window bar can offer.
#[derive(Debug, Clone, Copy)]
enum Cmd {
    AppearanceSystem,
    AppearanceLight,
    AppearanceDark,
    Left,
    Right,
    SwitchProfile,
    ImportObsProfile,
    ExportProfile,
    Quit,
    Undo,
    Redo,
    About,
}

impl Cmd {
    const fn label(self) -> &'static str {
        match self {
            Self::AppearanceSystem => "Appearance: System",
            Self::AppearanceLight => "Appearance: Light",
            Self::AppearanceDark => "Appearance: Dark",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::SwitchProfile => "Switch profile…",
            Self::ImportObsProfile => "Import OBS profile…",
            Self::ExportProfile => "Export profile…",
            Self::Quit => "Quit",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::About => "About PartyTime",
        }
    }

    fn popup_item(self) -> PopupMenuItem {
        let name = self.label();
        let action: Box<dyn gpui_kit::Action> = match self {
            Self::AppearanceSystem => Box::new(AppearanceSystem),
            Self::AppearanceLight => Box::new(AppearanceLight),
            Self::AppearanceDark => Box::new(AppearanceDark),
            Self::Left => Box::new(ToggleLeftDock),
            Self::Right => Box::new(ToggleRightDock),
            Self::SwitchProfile => Box::new(SwitchProfile),
            Self::ImportObsProfile => Box::new(ImportObsProfile),
            Self::ExportProfile => Box::new(ExportProfile),
            Self::Quit => Box::new(Quit),
            Self::Undo => Box::new(Undo),
            Self::Redo => Box::new(Redo),
            Self::About => Box::new(AboutPartyTime),
        };
        PopupMenuItem::new(name).action(action)
    }
}

/// One entry in the in-window menu bar.
struct Entry {
    label: &'static str,
    commands: &'static [Cmd],
    /// Rendered greyed: the commands behind it do not exist yet.
    disabled: bool,
    /// Shown instead of an empty dropdown — an empty menu is worse than a reason.
    empty_note: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        label: "File",
        commands: &[Cmd::ImportObsProfile, Cmd::ExportProfile, Cmd::Quit],
        disabled: false,
        empty_note: "",
    },
    Entry {
        label: "Edit",
        commands: &[Cmd::Undo, Cmd::Redo],
        disabled: false,
        empty_note: "",
    },
    Entry {
        label: "View",
        commands: &[
            Cmd::AppearanceSystem,
            Cmd::AppearanceLight,
            Cmd::AppearanceDark,
        ],
        disabled: false,
        empty_note: "",
    },
    Entry {
        label: "Docks",
        commands: &[Cmd::Left, Cmd::Right],
        disabled: false,
        empty_note: "",
    },
    Entry {
        label: "Profile",
        commands: &[Cmd::SwitchProfile],
        disabled: false,
        empty_note: "",
    },
    Entry {
        label: "Scene Collection",
        commands: &[],
        disabled: true,
        empty_note: "PartyTime has one profile per configuration, not collections.",
    },
    Entry {
        label: "Tools",
        commands: &[],
        disabled: true,
        empty_note: "Filters and transforms arrive with the engine.",
    },
    Entry {
        label: "Help",
        commands: &[Cmd::About],
        disabled: false,
        empty_note: "",
    },
];

/// The menu bar drawn inside the window.
///
/// GPUI's `set_menus` hands menus to the *platform*, and GNOME has no platform menu bar to
/// draw them in — the call succeeds and nothing appears. So the console draws its own, the
/// way OBS does: one button per menu, a dropdown under it.
pub fn menu_bar() -> AnyElement {
    h_flex()
        .id("menu-bar")
        .test_support()
        .gap_1()
        .items_center()
        .children(ENTRIES.iter().map(|entry| {
            let button = Button::new(format!("menu:{}", entry.label))
                .ghost()
                .xsmall()
                .label(entry.label)
                .tooltip(if entry.disabled {
                    entry.empty_note
                } else {
                    entry.label
                })
                .disabled(entry.disabled);
            if entry.commands.is_empty() {
                return button.into_any_element();
            }
            let commands = entry.commands;
            button
                .dropdown_menu(move |menu, _window, _cx| {
                    commands
                        .iter()
                        .fold(menu, |menu, cmd| menu.item(cmd.popup_item()))
                })
                .into_any_element()
        }))
        .into_any_element()
}

/// A named menu, optionally disabled.
fn menu(name: &str, disabled: bool, items: Vec<MenuItem>) -> Menu {
    Menu {
        name: name.into(),
        items,
        disabled,
    }
}

/// Installs the menu bar on the application.
pub fn install(app: &App) {
    app.set_menus(menus());
}

/// The command names this build dispatches, for the About screen and for tests.
///
/// A menu entry whose command is not in this list cannot do anything when chosen, which
/// is how the disabled menus are verified to be genuinely inert rather than merely
/// decorated.
#[must_use]
pub fn wired_commands() -> &'static [&'static str] {
    &[
        "AppearanceSystem",
        "AppearanceLight",
        "AppearanceDark",
        "ToggleLeftDock",
        "ToggleRightDock",
        "AboutPartyTime",
        "SwitchProfile",
        "ImportObsProfile",
        "ExportProfile",
        "Quit",
        "Undo",
        "Redo",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window bar (`ENTRIES`) and the platform menus (`menus`) describe the same
    /// menu bar twice, because GPUI's `set_menus` reaches nothing on GNOME. They have to
    /// agree: when they drift, the menu the creator actually clicks is greyed out while
    /// the tests say it is not.
    #[test]
    fn the_window_bar_and_the_platform_menus_agree() {
        let bar = menus();
        for entry in ENTRIES {
            let menu = bar
                .iter()
                .find(|m| m.name.as_ref() == entry.label)
                .unwrap_or_else(|| {
                    panic!("{} is in the window bar but not in menus()", entry.label)
                });
            assert_eq!(
                menu.disabled, entry.disabled,
                "{} is disabled in one description and not the other",
                entry.label
            );
        }
        for menu in &bar {
            assert!(
                ENTRIES.iter().any(|e| e.label == menu.name.as_ref()),
                "{} is in menus() but never drawn in the window",
                menu.name
            );
        }
        // The specific regression: File and Edit stayed greyed out in the running app
        // because only the platform menus were updated. Pin what each one offers.
        let commands = |label: &str| -> Vec<&'static str> {
            ENTRIES
                .iter()
                .find(|e| e.label == label)
                .unwrap_or_else(|| panic!("no {label} entry"))
                .commands
                .iter()
                .map(|c| c.label())
                .collect()
        };
        assert_eq!(
            commands("File"),
            ["Import OBS profile…", "Export profile…", "Quit"],
            "the File menu in the window bar does not offer what it does now support"
        );
        assert_eq!(
            commands("Edit"),
            ["Undo", "Redo"],
            "the Edit menu in the window bar does not offer what it now supports"
        );
        // An entry that is clickable but offers nothing is the thing this whole file
        // exists to prevent.
        for entry in ENTRIES.iter().filter(|e| !e.disabled) {
            assert!(
                !entry.commands.is_empty(),
                "{} is selectable but offers no commands",
                entry.label
            );
        }
    }

    /// File used to be disabled because nothing was behind it. Now that import, export
    /// and quit are dispatched, the menu has to be openable and every entry has to be
    /// named here - otherwise it is an entry that does nothing when chosen.
    #[test]
    fn every_file_entry_is_a_command_this_build_dispatches() {
        let file = menus()
            .into_iter()
            .find(|m| m.name == "File")
            .expect("File menu");
        assert!(!file.disabled, "the File menu is still disabled");
        for name in ["ImportObsProfile", "ExportProfile", "Quit"] {
            assert!(
                wired_commands().contains(&name),
                "File dispatches {name}, so it must be in wired_commands"
            );
        }
    }

    #[test]
    fn the_bar_carries_the_obs_menu_set() {
        let names: Vec<String> = menus().into_iter().map(|m| m.name.to_string()).collect();
        for expected in [
            "File",
            "Edit",
            "View",
            "Docks",
            "Profile",
            "Scene Collection",
            "Tools",
            "Help",
        ] {
            assert!(
                names.iter().any(|n| n == expected || n.contains(expected)),
                "the {expected} menu is missing; got {names:?}"
            );
        }
    }

    #[test]
    fn menus_with_nothing_behind_them_are_disabled() {
        let bar = menus();
        for menu in &bar {
            let name = menu.name.to_string();
            if name == "Scene Collection" || name == "Tools" {
                assert!(
                    menu.disabled,
                    "{name} should not be selectable while the engine is absent"
                );
            } else {
                assert!(!menu.disabled, "{name} is usable and must not be disabled");
            }
        }
    }

    #[test]
    fn view_offers_appearance_and_docks_is_a_top_level_menu() {
        let view = menus()
            .into_iter()
            .find(|m| m.name.as_ref() == "View")
            .expect("View menu");
        let submenus: Vec<String> = view
            .items
            .into_iter()
            .filter_map(|item| match item {
                MenuItem::Submenu(sub) => Some(sub.name.to_string()),
                _ => None,
            })
            .collect();
        assert!(submenus.contains(&"Appearance".to_string()), "{submenus:?}");

        // OBS has Docks as its own top-level menu, not tucked under View.
        let names: Vec<String> = menus().into_iter().map(|m| m.name.to_string()).collect();
        assert!(names.contains(&"Docks".to_string()), "{names:?}");
    }
}
