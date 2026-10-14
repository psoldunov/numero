//! The macOS menu bar: the app menu, Edit and View. Each item carries the
//! action its shortcut runs, so the menus show the keys. Linux has no
//! global menu bar, so only macOS sets them, but they build everywhere.

use gpui::{App, Menu, MenuItem};
use gpui_component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};

use crate::app::{Quit, ResetZoom, ZoomIn, ZoomOut};

pub(crate) fn set(cx: &mut App) {
    cx.set_menus(menus());
}

fn menus() -> Vec<Menu> {
    vec![
        Menu::new("Numero").items([MenuItem::action("Quit Numero", Quit)]),
        Menu::new("Edit").items([
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
            MenuItem::separator(),
            MenuItem::action("Cut", Cut),
            MenuItem::action("Copy", Copy),
            MenuItem::action("Paste", Paste),
            MenuItem::action("Select All", SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Actual Size", ResetZoom),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_bar_has_the_app_edit_and_view_menus() {
        let names: Vec<_> = menus().into_iter().map(|menu| menu.name).collect();
        assert_eq!(names, ["Numero", "Edit", "View"]);
    }
}
