//! The app's place in the desktop: the menu-bar (tray) item, the global
//! shortcut, opening at login, staying on top, the window's saved frame and
//! the single running instance.
//!
//! Contract with the rest of the app: the preferences call
//! [`settings_changed`] after they save, and read and write the login item
//! through [`launch_at_login`] and [`set_launch_at_login`], which ask the
//! system rather than the settings file (the system's list keeps the
//! choice, as Numi's helper does).

// Stubs until the preferences call them; remove once wired.
#![allow(dead_code)]

use gpui::App;

use crate::settings::Settings;

/// Apply the parts of the settings this module owns (menu-bar mode, always
/// on top, the global shortcut) after they changed.
pub(crate) fn settings_changed(_old: &Settings, _new: &Settings, _cx: &mut App) {}

/// Whether Numero opens when the user logs in.
pub(crate) fn launch_at_login() -> bool {
    false
}

/// Add Numero to, or remove it from, the programs that open at login.
pub(crate) fn set_launch_at_login(_enabled: bool) -> Result<(), String> {
    Err("not implemented yet".into())
}
