//! Whether Numero opens when the user logs in, read from and written to the
//! system's own list, so the preferences and the system never disagree, as
//! Numi's login helper does (`docs/numi-spec.md` §12.10): an XDG autostart
//! entry on Linux, the app's `SMAppService` login item on macOS.
//!
//! A launch also knows whether the login started it: the autostart entry
//! passes [`AUTOSTART_OPTION`], and macOS marks the login item's launch
//! event. Such a launch starts in the tray when there is one.

#[cfg(target_os = "linux")]
mod autostart;
#[cfg(target_os = "macos")]
mod macos;

use std::ffi::OsString;

#[cfg(target_os = "linux")]
pub(super) use autostart::{is_on, set, upgrade};
#[cfg(target_os = "macos")]
pub(super) use macos::{is_on, launched_as_login_item, set};

/// The option the autostart entry starts Numero with.
pub(super) const AUTOSTART_OPTION: &str = "--autostart";

/// Whether the command line `args` (the program's name left out) is the
/// autostart entry's.
pub(super) fn autostarted(args: &[OsString]) -> bool {
    args.iter().any(|arg| arg == AUTOSTART_OPTION)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) const fn is_on() -> bool {
    false
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn set(_on: bool) -> Result<(), String> {
    Err("opening at login isn't supported on this system".to_owned())
}

/// Brings an autostart entry an older Numero wrote up to date; only Linux
/// has one.
#[cfg(not(target_os = "linux"))]
pub(super) const fn upgrade() {}

/// Whether macOS started this launch as the login item; only macOS has
/// one.
#[cfg(not(target_os = "macos"))]
pub(super) const fn launched_as_login_item() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_autostart_option_marks_a_launch_at_login() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        assert!(autostarted(&args(&["--autostart"])));
        assert!(autostarted(&args(&["sums.txt", "--autostart"])));
        assert!(!autostarted(&args(&[])));
        assert!(!autostarted(&args(&["--autostart=no", "autostart"])));
    }
}
