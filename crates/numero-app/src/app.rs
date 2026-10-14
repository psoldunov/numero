//! Starting the app: gpui-component, the theme, the shortcuts and the
//! calculator's window.
//!
//! One Numero runs at a time: a second launch hands what it was asked to
//! open to the first and exits (`system`). Ctrl+Q or ⌘Q ends the app;
//! closing the window ends it only on Linux, and only while no tray item
//! could bring the window back. Editing keys (copy, paste, undo, select
//! all) come from gpui-component's editor, which binds them with Ctrl on
//! Linux and ⌘ on macOS.

use gpui::{
    AnyWindowHandle, App, AppContext as _, DisplayId, KeyBinding, QuitMode, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, actions,
};
use gpui_component::Theme;

use crate::calculator::{Calculator, Sources};
use crate::settings::SettingsFile;
use crate::system::{self, Launch};
use crate::{frame, menus};

actions!(numero, [Quit, ZoomIn, ZoomOut, ResetZoom]);

const TITLE: &str = "Numero";
/// The app's id: its Wayland app id, its tray item's, its autostart entry's
/// and its single instance's socket's name.
/// Reverse-DNS, as the XDG GlobalShortcuts portal requires; also the Wayland
/// app id, so the installed desktop file is `dev.soldunov.numero.desktop`.
pub(crate) const APP_ID: &str = "dev.soldunov.numero";
/// The icon theme name the packages install the app icon under.
pub(crate) const ICON_NAME: &str = "numero";

/// Runs the app until the user quits, unless Numero runs already: then
/// this launch shows it, hands it what to open, and returns.
pub(crate) fn run() {
    let Some(launch) = system::launch() else {
        return;
    };
    // Closing the window doesn't end the app by itself: `system` decides.
    let application = gpui::application()
        .with_assets(crate::assets::Assets)
        .with_quit_mode(QuitMode::Explicit);
    launch.listen(&application);
    application.run(move |cx| start(launch, cx));
}

fn start(launch: Launch, cx: &mut App) {
    // The frame's plugins go either side of gpui-component's own, which
    // would otherwise wrap the window in its square border.
    frame::register_capture(cx);
    gpui::init(cx);
    frame::register_frame(cx);
    Theme::sync_system_appearance(None, cx);
    cx.bind_keys(key_bindings());
    menus::init(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    if cfg!(target_os = "macos") {
        menus::set(cx);
    }
    let settings = SettingsFile::from_env()
        .map(|file| file.load())
        .unwrap_or_default();
    // Opens the window, at its saved frame.
    system::start(launch, &settings, cx);
}

/// The app's own shortcuts. `secondary` is ⌘ on macOS and Ctrl elsewhere.
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary-+", ZoomIn, None),
        KeyBinding::new("secondary-shift-=", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
        KeyBinding::new("secondary-0", ResetZoom, None),
    ]
}

/// Opens the calculator's window at `bounds` on `display`, on the
/// calculation open last.
pub(crate) fn open_window(
    bounds: WindowBounds,
    display: Option<DisplayId>,
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let sources = Sources::from_env();
    let options = WindowOptions {
        window_bounds: Some(bounds),
        display_id: display,
        ..window_options()
    };
    let opened = gpui::open_window(options, cx, |window, cx| {
        // Joined now, so the first frame knows whether the desktop blurs.
        frame::connect(window, cx);
        cx.new(|cx| {
            let mut calculator = Calculator::new(sources, window, cx);
            calculator.start_rates(cx);
            cx.observe_window_bounds(window, |_, window, cx| system::frame_changed(window, cx))
                .detach();
            calculator
        })
    });
    match opened {
        Ok((window, _)) => Some(window),
        Err(error) => {
            tracing::error!(%error, "couldn't open the window");
            cx.quit();
            None
        }
    }
}

fn window_options() -> WindowOptions {
    WindowOptions {
        window_min_size: Some(system::MIN_WINDOW_SIZE),
        app_id: Some(APP_ID.to_owned()),
        ..platform_window_options()
    }
}

/// The window draws its own title bar and rounded frame, so it looks the
/// same on every desktop, GNOME included, which draws none. The surface is
/// transparent: the frame paints the translucent background, and asks the
/// compositor to blur behind it where it can (`KWin` on Wayland).
#[cfg(not(target_os = "macos"))]
fn platform_window_options() -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            ..TitlebarOptions::default()
        }),
        window_decorations: Some(gpui::WindowDecorations::Client),
        window_background: WindowBackgroundAppearance::Transparent,
        app_owns_titlebar_drag: true,
        ..WindowOptions::default()
    }
}

/// AppKit draws the frame, its corners and shadow, and the traffic lights,
/// which sit in the app's own title bar (concierge's ADR 0063). The window
/// is transparent and the window server blurs behind it. The title bar
/// moves the window and answers a double click itself, as on Linux.
#[cfg(target_os = "macos")]
fn platform_window_options() -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            appears_transparent: true,
            traffic_light_position: Some(crate::title_bar::TRAFFIC_LIGHTS),
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        app_owns_titlebar_drag: true,
        ..WindowOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_parses() {
        // `KeyBinding::new` panics on a keystroke it can't parse.
        assert_eq!(key_bindings().len(), 6);
    }
}
