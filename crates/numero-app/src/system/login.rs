//! Whether Numero opens when the user logs in, read from and written to the
//! system's own list, so the preferences and the system never disagree, as
//! Numi's login helper does (`docs/numi-spec.md` §12.10): an XDG autostart
//! entry on Linux, the app's `SMAppService` login item on macOS.

#[cfg(target_os = "linux")]
mod autostart;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub(super) use autostart::{is_on, set};
#[cfg(target_os = "macos")]
pub(super) use macos::{is_on, set};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) const fn is_on() -> bool {
    false
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn set(_on: bool) -> Result<(), String> {
    Err("opening at login isn't supported on this system".to_owned())
}
