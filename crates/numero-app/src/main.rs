//! Numero's app for Linux and macOS: a text calculator in the manner of
//! Numi, on GPUI with gpui-component.
//!
//! - `app`: starting up, the shortcuts and the window.
//! - `calculator`: the view's state: the editor, the engine, the results,
//!   saving and copying.
//! - `view`, `results` and `follow`: how the window is drawn, the results
//!   column, and the element that keeps it level with the editor's lines.
//! - `document`: the engine's answers as editor ranges and result rows.
//! - `theme`: the light and dark colours; `ui_font`: the desktop's sans
//!   family for the UI text.
//! - `library`: the calculations on disk; `settings`: the settings file;
//!   `export`: a calculation as text for the Export menu.
//! - `calculations_list`, `settings_panel`, `footer` and `widgets`: the list
//!   of calculations, the settings, the bottom corners and their controls.
//! - `assets`: the icons.
//! - `rates`: cached and fetched currency rates.
//! - `frame`, `title_bar` and `blur`: the window's own rounded frame and
//!   title bar, and the blur behind its translucent background.
//! - `menus`: the macOS menu bar.
//! - `system`: tray, global shortcut, login item, always on top, window
//!   frame and single instance.

mod app;
mod assets;
mod blur;
mod calculations_list;
mod calculator;
mod document;
mod export;
mod follow;
mod footer;
mod frame;
mod library;
mod menus;
mod rates;
mod results;
mod settings;
mod settings_panel;
mod system;
mod theme;
mod title_bar;
mod ui_font;
mod view;
mod widgets;

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();
    app::run();
}
