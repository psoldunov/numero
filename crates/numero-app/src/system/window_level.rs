//! Keeping a window above the others, which GPUI can't do after the window
//! is open: Numi's "Always on top" puts the window at the status-window
//! level on macOS (`docs/numi-spec-ui.md` §6.2). X11 has
//! `_NET_WM_STATE_ABOVE`; Wayland has no protocol a window can ask with, so
//! there the setting does nothing and says so in the log.
//!
//! On macOS the window also follows the user to the Space they are on when
//! shown, as Numi's `moveToActiveSpace`.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Puts `window` above every normal window, or back among them.
pub(crate) fn keep_on_top(window: &Window, on: bool) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    match handle.as_raw() {
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xcb(handle) => x11::keep_above(handle.window.get(), on),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(handle) => match u32::try_from(handle.window) {
            Ok(window) => x11::keep_above(window, on),
            Err(_) => tracing::warn!("an X11 window id out of range"),
        },
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(handle) => macos::keep_on_top(handle.ns_view, on),
        RawWindowHandle::Wayland(_) if on => {
            tracing::info!(
                "Wayland lets no window keep itself on top; ask the compositor with a window rule"
            );
        }
        _ => {}
    }
}

/// Whether `window` is an X11 one.
pub(crate) fn is_x11(window: &Window) -> bool {
    HasWindowHandle::window_handle(window).is_ok_and(|handle| {
        matches!(
            handle.as_raw(),
            RawWindowHandle::Xcb(_) | RawWindowHandle::Xlib(_)
        )
    })
}

/// Moves an X11 `window`'s outer corner to `origin`, in GPUI's logical
/// pixels. Does nothing elsewhere.
pub(crate) fn move_to(window: &Window, origin: gpui::Point<gpui::Pixels>) {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    #[cfg(target_os = "linux")]
    {
        let id = match handle.as_raw() {
            RawWindowHandle::Xcb(handle) => Some(handle.window.get()),
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).ok(),
            _ => None,
        };
        if let Some(id) = id {
            let scale = window.scale_factor();
            x11::move_to(id, origin.x.as_f32() * scale, origin.y.as_f32() * scale);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (handle, origin);
}

/// Whether the platform reports where `window` is: not on Wayland.
pub(crate) fn knows_position(window: &Window) -> bool {
    HasWindowHandle::window_handle(window)
        .is_ok_and(|handle| !matches!(handle.as_raw(), RawWindowHandle::Wayland(_)))
}

/// Makes `window` come to the Space the user is on when it is shown, rather
/// than taking the user to the Space it was left on.
#[cfg(target_os = "macos")]
pub(crate) fn follow_active_space(window: &Window) {
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        macos::follow_active_space(handle.ns_view);
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{
        ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask,
    };

    /// `_NET_WM_STATE`'s actions.
    const REMOVE: u32 = 0;
    const ADD: u32 = 1;

    /// "A normal application" asks, per the EWMH.
    const SOURCE_APPLICATION: u32 = 1;

    /// Asks the window manager to keep `window` above the others, or not.
    pub(super) fn keep_above(window: u32, on: bool) {
        if let Err(error) = ask(window, on) {
            tracing::warn!(%error, "the window manager didn't take always on top");
        }
    }

    /// Moves `window` to `x`, `y` in device pixels.
    pub(super) fn move_to(window: u32, x: f32, y: f32) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a place on screen fits in an i32"
        )]
        let (x, y) = (x.round() as i32, y.round() as i32);
        let moved = x11rb::connect(None)
            .map_err(|error| error.to_string())
            .and_then(|(connection, _)| {
                let place = ConfigureWindowAux::new().x(x).y(y);
                connection
                    .configure_window(window, &place)
                    .map_err(|error| error.to_string())?;
                connection.flush().map_err(|error| error.to_string())
            });
        match moved {
            Ok(()) => tracing::info!(x, y, "put the window back where it was"),
            Err(error) => tracing::warn!(%error, "couldn't put the window back where it was"),
        }
    }

    fn ask(window: u32, on: bool) -> Result<(), String> {
        let fail = |error: &dyn std::fmt::Display| error.to_string();
        let (connection, screen) = x11rb::connect(None).map_err(|error| fail(&error))?;
        let root = connection
            .setup()
            .roots
            .get(screen)
            .ok_or("no such X11 screen")?
            .root;
        let atom = |name: &[u8]| -> Result<u32, String> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(|error| fail(&error))?
                .reply()
                .map_err(|error| fail(&error))?
                .atom)
        };
        let state = atom(b"_NET_WM_STATE")?;
        let above = atom(b"_NET_WM_STATE_ABOVE")?;
        let action = if on { ADD } else { REMOVE };
        let event =
            ClientMessageEvent::new(32, window, state, [action, above, 0, SOURCE_APPLICATION, 0]);
        connection
            .send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )
            .map_err(|error| fail(&error))?;
        connection.flush().map_err(|error| fail(&error))?;
        tracing::info!(on, "asked the window manager to keep the window on top");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::NonNull;

    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSNormalWindowLevel, NSStatusWindowLevel, NSView, NSWindow, NSWindowCollectionBehavior,
    };

    /// Status-window level, as Numi's `CGWindowLevelForKey(9)`, or normal.
    pub(super) fn keep_on_top(view: NonNull<c_void>, on: bool) {
        if let Some(window) = window(view) {
            window.setLevel(if on {
                NSStatusWindowLevel
            } else {
                NSNormalWindowLevel
            });
        }
    }

    pub(super) fn follow_active_space(view: NonNull<c_void>) {
        if let Some(window) = window(view) {
            let behavior =
                window.collectionBehavior() | NSWindowCollectionBehavior::MoveToActiveSpace;
            window.setCollectionBehavior(behavior);
        }
    }

    /// The `NSWindow` of the `NSView` GPUI draws into.
    #[expect(
        unsafe_code,
        reason = "raw-window-handle hands the NSView over as a raw pointer"
    )]
    fn window(view: NonNull<c_void>) -> Option<Retained<NSWindow>> {
        // SAFETY: an AppKit window handle's `ns_view` points to the live
        // NSView of the window it came from, which outlives this call.
        let view = unsafe { view.cast::<NSView>().as_ref() };
        view.window()
    }
}
