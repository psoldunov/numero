//! The app's commands: one GPUI action each, their shortcuts, and the
//! macOS menu bar that lists them (`docs/numi-spec.md` §12.3).
//!
//! Every action is handled app-wide ([`handlers`]), so it works from a
//! shortcut, the menu bar, an in-window menu and the tray alike: the tray
//! dispatches the same actions with `cx.dispatch_action`. Linux has no
//! global menu bar; the shortcuts work there all the same, and the footer's
//! menu lists the commands that have no button.
//!
//! `secondary` is ⌘ on macOS and Ctrl elsewhere.

mod handlers;
pub(crate) mod samples;

use gpui::{Action, App, KeyBinding, Menu, MenuItem, actions};
use gpui_component::input::{Copy, Cut, Paste, Redo, Search, SelectAll, Undo};

use crate::app::{Quit, ResetZoom, ZoomIn, ZoomOut};

actions!(
    numero,
    [
        /// File → New: an empty calculation.
        NewCalculation,
        /// File → Import: a `.txt` or `.numi` file as a new calculation.
        ImportFile,
        /// File → Export: the calculation saved to a file the user picks.
        ExportFile,
        /// The export menu's "Copy to Clipboard": every line with its
        /// result, lined up.
        CopyToClipboard,
        /// File → List: the list of calculations.
        ShowList,
        /// File → Close: closes the window in front.
        CloseWindow,
        /// Edit → Copy Result: the caret line's result.
        CopyResult,
        /// Edit → Copy All: the lines with their results.
        CopyAll,
        /// Edit → Delete All: empties the calculation.
        DeleteAll,
        /// Edit → Surround with Parentheses.
        SurroundWithParentheses,
        /// Numero → Preferences.
        OpenPreferences,
        /// Help → Syntax Guide: a new calculation explaining the syntax.
        OpenDocs,
        /// Opens the folder JavaScript extensions load from.
        OpenExtensionsFolder,
        /// Window → Minimize.
        Minimize,
        /// Window → Zoom.
        ZoomWindow,
        /// Numero → Hide Numero, Hide Others and Show All (macOS).
        HideApp,
        HideOthers,
        ShowAll,
    ]
);

/// Help → Samples: the sample at this index of [`samples::SAMPLES`] as a
/// new calculation.
#[derive(Debug, Clone, PartialEq, Eq, Action)]
#[action(namespace = numero, no_json)]
pub(crate) struct OpenSample(pub usize);

/// Binds the shortcuts and handles every action app-wide. Call it once at
/// start, before the menus are set: they read their keys from the keymap.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
    handlers::register(cx);
}

/// The commands' shortcuts.
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("secondary-n", NewCalculation, None),
        KeyBinding::new("secondary-o", ImportFile, None),
        KeyBinding::new("secondary-s", ExportFile, None),
        KeyBinding::new("secondary-l", ShowList, None),
        KeyBinding::new("secondary-w", CloseWindow, None),
        KeyBinding::new("secondary-shift-c", CopyResult, None),
        KeyBinding::new("secondary-alt-c", CopyAll, None),
        KeyBinding::new("secondary-shift-0", SurroundWithParentheses, None),
        // Some keyboards report ⇧0 as the character it types.
        KeyBinding::new("secondary-)", SurroundWithParentheses, None),
        KeyBinding::new("secondary-,", OpenPreferences, None),
        KeyBinding::new("secondary-?", OpenDocs, None),
        KeyBinding::new("secondary-shift-/", OpenDocs, None),
        KeyBinding::new("secondary-m", Minimize, None),
    ];
    if cfg!(target_os = "macos") {
        bindings.extend([
            KeyBinding::new("cmd-alt-backspace", DeleteAll, None),
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
        ]);
    } else {
        // Ctrl+Alt+Backspace ends the session on some X11 setups.
        bindings.push(KeyBinding::new("ctrl-shift-backspace", DeleteAll, None));
    }
    bindings
}

/// Puts the menus in the macOS menu bar.
pub(crate) fn set(cx: &mut App) {
    cx.set_menus(menus());
}

fn menus() -> Vec<Menu> {
    vec![
        Menu::new("Numero").items([
            MenuItem::action("Preferences…", OpenPreferences),
            MenuItem::separator(),
            MenuItem::action("Hide Numero", HideApp),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Numero", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New", NewCalculation),
            MenuItem::action("Import…", ImportFile),
            MenuItem::action("Export…", ExportFile),
            MenuItem::separator(),
            MenuItem::action("List", ShowList),
            MenuItem::separator(),
            MenuItem::action("Close", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
            MenuItem::separator(),
            MenuItem::action("Cut", Cut),
            MenuItem::action("Copy", Copy),
            MenuItem::action("Copy Result", CopyResult),
            MenuItem::action("Copy All", CopyAll),
            MenuItem::action("Paste", Paste),
            MenuItem::action("Select All", SelectAll),
            MenuItem::action("Delete All", DeleteAll),
            MenuItem::action("Surround with Parentheses", SurroundWithParentheses),
            MenuItem::separator(),
            MenuItem::action("Find…", Search),
        ]),
        Menu::new("View").items([
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ResetZoom),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", ZoomWindow),
        ]),
        Menu::new("Help").items([
            MenuItem::action("Syntax Guide", OpenDocs),
            MenuItem::submenu(Menu::new("Samples").items(sample_items())),
            MenuItem::separator(),
            MenuItem::action("Open Extensions Folder", OpenExtensionsFolder),
        ]),
    ]
}

fn sample_items() -> Vec<MenuItem> {
    samples::SAMPLES
        .iter()
        .enumerate()
        .map(|(index, sample)| MenuItem::action(sample.name, OpenSample(index)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_parses() {
        // `KeyBinding::new` panics on a keystroke it can't parse.
        assert!(key_bindings().len() >= 14);
    }

    #[test]
    fn the_menu_bar_follows_numis_order() {
        let names: Vec<_> = menus().into_iter().map(|menu| menu.name).collect();
        assert_eq!(names, ["Numero", "File", "Edit", "View", "Window", "Help"]);
    }

    #[test]
    fn help_offers_every_sample() {
        assert_eq!(sample_items().len(), samples::SAMPLES.len());
    }
}
