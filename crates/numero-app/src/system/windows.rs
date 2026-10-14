//! Showing and hiding the calculator's window, as Numi's toggle does
//! (`docs/numi-spec.md` §12.4): a window in front hides, and the app the
//! user was in comes back; otherwise the window comes forward on the
//! current Space or workspace and takes the keyboard.
//!
//! macOS hides the app, which hands the front back to the app before it,
//! and keeps the window. GPUI can't hide a window on Linux, so there hiding
//! closes it, and showing opens it again on the calculation open last;
//! the calculation is saved first.
//!
//! Closing the window ends the app only on Linux, and only while no tray
//! item could bring it back. On macOS the app keeps running, as Numi does,
//! and the Dock icon or the tray shows the window again.

use std::time::Duration;

use gpui::{
    AnyWindowHandle, App, Bounds, DisplayId, Pixels, Point, Window, WindowBounds, WindowId,
};

use super::Desktop;
use super::saved_frame::{self, Frame, SavedFrame, Screen};
use super::window_level;
use crate::calculator::main_window;

/// How long after the window last moved its frame is saved.
pub(super) const SAVE_FRAME_AFTER: Duration = Duration::from_millis(400);

/// How long an X11 window manager takes to place a new window.
const PLACED_AFTER: Duration = Duration::from_millis(250);

/// Shows the window, or hides it if it is in front.
pub(super) fn toggle(cx: &mut App) {
    let in_front = main_window::window(cx).is_some_and(|window| {
        window
            .update(cx, |_, window, _| window.is_window_active())
            .unwrap_or(false)
    });
    tracing::info!(in_front, "toggling the window");
    if in_front {
        hide(cx);
    } else {
        show(cx);
    }
}

/// Brings the window forward, opening it if it is closed.
pub(crate) fn show(cx: &mut App) {
    // On macOS a window comes forward only in the active app, and the
    // tray, the shortcut and a second launch reach Numero while another
    // app is active; activating also undoes hiding.
    cx.activate(true);
    if let Some(window) = main_window::window(cx)
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    tracing::info!("opening the window");
    open(cx);
}

/// Hides the window and hands the front back to the app before it.
fn hide(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.hide();
        return;
    }
    let Some(window) = main_window::window(cx) else {
        return;
    };
    // Closing drops the calculator with the save it had scheduled.
    if let Some(calculator) = main_window::calculator(cx) {
        calculator.update(cx, |calculator, cx| calculator.save_now(cx));
    }
    cx.global_mut::<Desktop>().hiding = true;
    let closed = window.update(cx, |_, window, _| window.remove_window());
    if closed.is_err() {
        cx.global_mut::<Desktop>().hiding = false;
    }
}

/// Opens the window where it was last, or centred at Numi's size.
fn open(cx: &mut App) {
    let saved = cx
        .global::<Desktop>()
        .frame_file
        .as_ref()
        .and_then(saved_frame::FrameFile::load);
    let screens: Vec<Screen> = cx
        .displays()
        .iter()
        .map(|display| Screen {
            id: u64::from(display.id()),
            bounds: display.bounds(),
        })
        .collect();
    let placement = saved_frame::place(saved.as_ref(), &screens);
    let display = placement.display.map(DisplayId::from);
    let bounds = match placement.frame {
        Frame::At(bounds) => bounds,
        Frame::Centered(size) => Bounds::centered(display, size, cx),
    };
    let bounds = if placement.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    let Some(window) = crate::app::open_window(bounds, display, cx) else {
        return;
    };
    if let Frame::At(saved) = placement.frame {
        put_back_once_placed(window, saved.origin, cx);
    }
    let placed = window.update(cx, |_, window, _| {
        #[cfg(target_os = "macos")]
        window_level::follow_active_space(window);
        window.window_handle().window_id()
    });
    if let Ok(id) = placed {
        cx.global_mut::<Desktop>().main_window = Some(id);
    }
}

/// Moves the window back to `origin` once the window manager has placed
/// it: KWin and other X11 window managers place a new window themselves,
/// whatever place it asked for. Nothing to do elsewhere: Wayland has no
/// places, and macOS keeps the one asked for.
fn put_back_once_placed(window: AnyWindowHandle, origin: Point<Pixels>, cx: &mut App) {
    if !cfg!(target_os = "linux") {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(PLACED_AFTER).await;
        let _closed = window.update(cx, |_, window, _| window_level::move_to(window, origin));
    })
    .detach();
}

/// Keeps `window` on top once the window manager has it: X11 window
/// managers ignore the request for a window not yet mapped.
pub(super) fn keep_on_top_once_shown(window: &mut Window) {
    window.on_next_frame(|window, _| window_level::keep_on_top(window, true));
}

/// Keeps every window of the app above the others, or not: the
/// calculator's and the preferences', as Numi does.
pub(super) fn set_always_on_top(on: bool, cx: &mut App) {
    for window in cx.windows() {
        let applied = window.update(cx, |_, window, _| window_level::keep_on_top(window, on));
        if let Err(error) = applied {
            tracing::debug!(%error, "a window closed before it could stay on top");
        }
    }
}

/// After a window closed: the calculator's, closed by the user, ends the
/// app on Linux unless the tray item could bring it back.
pub(super) fn closed(cx: &mut App, id: WindowId) {
    let desktop = cx.global_mut::<Desktop>();
    if desktop.main_window != Some(id) {
        return;
    }
    desktop.main_window = None;
    if std::mem::take(&mut desktop.hiding) {
        return;
    }
    let quit = cfg!(not(target_os = "macos")) && !desktop.tray_shown;
    if quit {
        tracing::info!("the window closed; quitting");
        cx.quit();
    }
}

/// The frame of `window` as it is kept: its place only where the platform
/// reports one.
///
/// Each platform reports the size a window was opened at differently: X11
/// grows the window by its shadow (the client inset) and reports that as
/// its bounds, the size asked for being the inner bounds; Wayland reports
/// the size asked for as the bounds, and its inner bounds are smaller
/// still. The place is the outer corner on X11, where alone it is known.
pub(super) fn frame_of(window: &Window, cx: &App) -> SavedFrame {
    let (bounds, maximized) = unpack(window.window_bounds());
    let size = if window_level::is_x11(window) {
        unpack(window.inner_window_bounds()).0.size
    } else {
        bounds.size
    };
    let positioned = window_level::knows_position(window);
    SavedFrame {
        width: size.width.as_f32(),
        height: size.height.as_f32(),
        x: positioned.then(|| bounds.origin.x.as_f32()),
        y: positioned.then(|| bounds.origin.y.as_f32()),
        display: window.display(cx).map(|display| u64::from(display.id())),
        maximized,
    }
}

fn unpack(bounds: WindowBounds) -> (Bounds<Pixels>, bool) {
    match bounds {
        WindowBounds::Windowed(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) => (bounds, true),
    }
}
