//! Blur behind the window's translucent background, where the desktop
//! offers it. The same approach as concierge's `ui/background_blur.rs`.
//!
//! On Linux the compositor blurs, through ext-background-effect-v1, and only
//! behind the shape the app asks for: `KWin` 6.7 on Wayland does; GNOME,
//! Sway and X11 don't, and the window paints itself opaque there. On macOS
//! the window server blurs behind the whole transparent window once asked,
//! through `CGSSetWindowBackgroundBlurRadius`; GPUI's own `Blurred` draws no
//! blur on current macOS. [`available`] is false there while the user asks
//! macOS to reduce transparency.

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub(crate) use linux::{Blur, available, connect, refresh};

#[cfg(target_os = "macos")]
pub(crate) use window_server::{Blur, available, connect, refresh};

/// Neither Linux nor macOS: no blur, and an opaque window.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) use none::{Blur, available, connect, refresh};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod none {
    use gpui::{App, Bounds, Corners, Pixels, Window};

    pub(crate) const fn connect(_: &Window, _: &mut App) {}

    pub(crate) const fn available(_: &App) -> bool {
        false
    }

    pub(crate) const fn refresh(_: &mut App) -> bool {
        false
    }

    #[derive(Debug)]
    pub(crate) struct Blur;

    impl Blur {
        pub(crate) const fn attach(_: &Window, _: &mut App) -> Option<Self> {
            None
        }

        pub(crate) const fn set(&mut self, _: Bounds<Pixels>, _: Corners<Pixels>, _: &App) {}
    }
}

/// The blur's API where the window server blurs behind a whole window.
#[cfg(target_os = "macos")]
mod window_server {
    use std::ptr::NonNull;

    use gpui::{App, Bounds, Corners, Global, Pixels, Window};
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindow, NSWorkspace};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// How far the window server spreads what is behind the window, in
    /// points: enough that text behind it can't be read through it.
    const RADIUS: i64 = 40;

    /// What [`available`] last answered, so [`refresh`] can tell when the
    /// user switched Reduce transparency.
    #[derive(Default)]
    struct Seen(Option<bool>);

    impl Global for Seen {}

    /// Nothing to join: each window asks the window server for its own blur.
    pub(crate) const fn connect(_: &Window, _: &mut App) {}

    /// Whether what is behind the window shows through, blurred: unless
    /// the user asked macOS to reduce transparency.
    pub(crate) fn available(_: &App) -> bool {
        !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
    }

    /// Whether [`available`] changed since it was last asked here.
    pub(crate) fn refresh(cx: &mut App) -> bool {
        let now = available(cx);
        let seen = cx.default_global::<Seen>();
        let changed = seen.0.is_some_and(|before| before != now);
        seen.0 = Some(now);
        changed
    }

    /// The blur behind the window: the whole of it, so there is no region
    /// to keep up to date.
    #[derive(Debug)]
    pub(crate) struct Blur;

    impl Blur {
        /// Asks the window server to blur behind `window`. `None` when the
        /// window isn't an AppKit one or the window server refused.
        pub(crate) fn attach(window: &Window, _: &mut App) -> Option<Self> {
            let number = native_window(window)?.windowNumber();
            match blur_behind(number) {
                0 => Some(Self),
                error => {
                    tracing::warn!(error, "the window server won't blur behind the window");
                    None
                }
            }
        }

        /// The window server blurs behind the whole window, whatever its
        /// shape: nothing to change.
        #[expect(
            clippy::unused_self,
            reason = "the same call as Linux's, whose blur keeps a region"
        )]
        pub(crate) const fn set(&mut self, _: Bounds<Pixels>, _: Corners<Pixels>, _: &App) {}
    }

    /// The `NSWindow` behind `window`. `None` when it isn't an AppKit one.
    fn native_window(window: &Window) -> Option<Retained<NSWindow>> {
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return None;
        };
        view(handle.ns_view).window()
    }

    /// The `NSView` GPUI draws the window into.
    #[expect(
        unsafe_code,
        reason = "raw-window-handle hands the NSView over as a raw pointer"
    )]
    fn view<'a>(pointer: NonNull<std::ffi::c_void>) -> &'a NSView {
        // SAFETY: an AppKit window handle's `ns_view` points to the live
        // NSView of the window it came from, which outlives this call.
        unsafe { pointer.cast::<NSView>().as_ref() }
    }

    /// Sets the blur behind the window numbered `window`, and answers the
    /// window server's error code: 0 when it took it.
    #[expect(
        unsafe_code,
        reason = "the window server's blur has no public API; GPUI called the same function \
                  before its NSVisualEffectView, as Kitty and Ghostty do"
    )]
    fn blur_behind(window: isize) -> i32 {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGSMainConnectionID() -> isize;
            fn CGSSetWindowBackgroundBlurRadius(
                connection: isize,
                window: isize,
                radius: i64,
            ) -> i32;
        }
        // SAFETY: both functions are exported by CoreGraphics on every
        // macOS the app supports, take plain integers, and read no memory
        // of ours.
        unsafe { CGSSetWindowBackgroundBlurRadius(CGSMainConnectionID(), window, RADIUS) }
    }
}
