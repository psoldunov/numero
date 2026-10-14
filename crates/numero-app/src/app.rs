//! Starting the app: gpui-component, the theme, the shortcuts and the one
//! window.
//!
//! The app ends when its window closes, on both platforms, and with
//! Ctrl+Q or ⌘Q. Editing keys (copy, paste, undo, select all) come from
//! gpui-component's editor, which binds them with Ctrl on Linux and ⌘ on
//! macOS.

use gpui::{
    App, AppContext as _, Bounds, KeyBinding, QuitMode, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, actions, px, size,
};
use gpui_component::Theme;

use crate::calculator::{Calculator, Sources};
use crate::library::Library;
use crate::settings::SettingsFile;
use crate::{frame, menus};

actions!(numero, [Quit, ZoomIn, ZoomOut, ResetZoom]);

const TITLE: &str = "Numero";
const APP_ID: &str = "numero";

/// Runs the app until the user quits.
pub(crate) fn run() {
    gpui::application()
        .with_assets(crate::assets::Assets)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(start);
}

fn start(cx: &mut App) {
    // The frame's plugins go either side of gpui-component's own, which
    // would otherwise wrap the window in its square border.
    frame::register_capture(cx);
    gpui::init(cx);
    frame::register_frame(cx);
    Theme::sync_system_appearance(None, cx);
    cx.bind_keys(key_bindings());
    cx.on_action(|_: &Quit, cx| cx.quit());
    if cfg!(target_os = "macos") {
        menus::set(cx);
    }
    open_window(cx);
    cx.activate(true);
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

fn open_window(cx: &mut App) {
    let library = Library::from_env();
    match &library {
        Some(library) => tracing::info!(dir = %library.dir().display(), "opening the calculations"),
        None => tracing::warn!("no data directory; calculations won't be saved"),
    }
    let sources = Sources {
        library,
        settings: SettingsFile::from_env(),
    };
    let opened = gpui::open_window(window_options(cx), cx, |window, cx| {
        // Joined now, so the first frame knows whether the desktop blurs.
        frame::connect(window, cx);
        cx.new(|cx| {
            let mut calculator = Calculator::new(sources, window, cx);
            calculator.start_rates(cx);
            calculator
        })
    });
    if let Err(error) = opened {
        tracing::error!(%error, "couldn't open the window");
        cx.quit();
    }
}

fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(370.), px(360.)),
            cx,
        ))),
        window_min_size: Some(size(px(300.), px(200.))),
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
