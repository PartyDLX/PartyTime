//! The PartyTime console binary.

use gpui_kit::component::TitleBar;
use gpui_kit::{AppContext as _, WindowOptions};
use studio_console::{AppShell, http, menu, theme};

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            // Must be first: it registers the themed window extension that `Root`
            // needs for dialogs, sheets and notifications.
            gpui_kit::init(cx);
            // GPUI ships no HTTP client — without this, every platform request fails
            // with "No HttpClient available" and sign-in cannot start. Install it before
            // anything can make a request.
            http::install(cx);
            // Native menus and their shortcuts, for platforms that draw them (macOS,
            // Windows). GNOME has no platform menu bar, which is why the shell also
            // draws its own; this call is what makes the shortcuts fire.
            menu::install(cx);
            // OpenParty's palette and Figtree, before anything measures text.
            theme::apply(cx);

            // Client-side title bar, so the window carries its own minimise, maximise
            // and close buttons and can host the menu bar.
            let options = WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                app_owns_titlebar_drag: true,
                ..WindowOptions::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| AppShell::new(window, cx))
            })
            .expect("failed to open the PartyTime window");
        });
}
