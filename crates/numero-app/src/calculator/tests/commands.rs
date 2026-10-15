//! The menus' commands on the calculator, and the bottom bar's total.

use gpui::{Entity, TestAppContext, VisualTestContext, px};

use super::{draw, kept_in, library_in, open, text, type_document, with_view};
use crate::calculator::exporting::read_text_file;
use crate::calculator::{Calculator, Sources};
use crate::menus::samples::SAMPLES;
use crate::menus::{CopyAll, DeleteAll, NewCalculation, OpenSample, SurroundWithParentheses};
use crate::settings::SettingsFile;

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

/// Puts the caret at the start of `line`.
fn caret_to_line(view: &Entity<Calculator>, line: u32, cx: &mut VisualTestContext) {
    let editor = view.read_with(cx, |calculator, _| calculator.editor.clone());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_cursor_position(gpui_component::input::Position::new(line, 0), window, cx);
        });
    });
}

/// Binds the shortcuts and the app-wide handlers, as the app does at start.
fn with_menus(cx: &mut VisualTestContext) {
    cx.update(|_, cx| crate::menus::init(cx));
}

#[gpui::test]
fn copy_all_puts_each_result_after_its_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    with_menus(cx);
    type_document(&view, "# Two\n1 + 1\n\n3 * 4", cx);

    cx.dispatch_action(CopyAll);
    cx.run_until_parked();

    assert_eq!(
        clipboard(cx).as_deref(),
        Some("# Two\n1 + 1 = 2\n\n3 * 4 = 12")
    );
}

#[gpui::test]
fn copy_result_copies_the_caret_lines_result(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    type_document(&view, "1\n2 + 3\n10", cx);
    caret_to_line(&view, 1, cx);

    with_view(&view, cx, |calculator, _, cx| {
        calculator.copy_result_at_caret(cx)
    });

    assert_eq!(clipboard(cx).as_deref(), Some("5"));
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.copied),
        Some(1)
    );
}

#[gpui::test]
fn surround_wraps_the_caret_lines_expression(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    with_menus(cx);
    type_document(&view, "x = 4 + 5\nx * 2", cx);
    caret_to_line(&view, 0, cx);

    cx.dispatch_action(SurroundWithParentheses);
    cx.run_until_parked();

    assert_eq!(text(&view, cx), "x = (4 + 5)\nx * 2");
}

#[gpui::test]
fn delete_all_empties_the_calculation(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    with_menus(cx);
    type_document(&view, "1 + 1\n2", cx);

    cx.dispatch_action(DeleteAll);
    cx.run_until_parked();

    assert_eq!(text(&view, cx), "");
    view.read_with(cx, |calculator, _| {
        assert!(calculator.rows.is_empty());
        assert_eq!(calculator.grand_total, None);
    });
}

#[gpui::test]
fn new_and_the_samples_open_new_calculations(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    with_menus(cx);
    let first = view.read_with(cx, |calculator, _| calculator.current.clone());

    cx.dispatch_action(OpenSample(1));
    cx.run_until_parked();
    assert_eq!(text(&view, cx), SAMPLES[1].text);

    cx.dispatch_action(NewCalculation);
    cx.run_until_parked();
    assert_eq!(text(&view, cx), "");
    assert_ne!(
        view.read_with(cx, |calculator, _| calculator.current.clone()),
        first
    );
    assert_eq!(library_in(dir.path()).entries().len(), 3);
}

#[gpui::test]
fn importing_a_text_file_makes_a_new_calculation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("budget.numi");
    std::fs::write(&file, "# Budget\r\n1200 + 300").unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));

    let imported = read_text_file(&file).unwrap();
    with_view(&view, cx, |calculator, window, cx| {
        calculator.open_new_with(&imported, window, cx);
    });

    assert_eq!(text(&view, cx), "# Budget\n1200 + 300");
    let titles: Vec<_> = library_in(dir.path())
        .entries()
        .into_iter()
        .map(|entry| entry.title)
        .collect();
    assert_eq!(titles.first(), Some(&Some("# Budget".to_owned())));
}

#[test]
fn import_refuses_what_isnt_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("picture.png");
    std::fs::write(&file, [0x89, b'P', b'N', b'G', 0, 0xff]).unwrap();
    assert!(
        read_text_file(&file)
            .unwrap_err()
            .contains("isn't a text file")
    );
}

#[gpui::test]
fn the_total_shows_only_when_the_calculation_has_one(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());

    type_document(&view, "# Notes\njust words", cx);
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.grand_total.clone()),
        None
    );

    type_document(&view, "2\n3", cx);
    let total = view.read_with(cx, |calculator, _| calculator.grand_total.clone());
    assert_eq!(total.as_deref(), Some("5"));
    draw(cx);

    with_view(&view, cx, |calculator, _, cx| calculator.copy_total(cx));
    assert_eq!(clipboard(cx).as_deref(), Some("5"));
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.footer_hint()),
        Some("Total copied".into())
    );
}

#[gpui::test]
fn a_dragged_result_pane_keeps_its_width(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let window_width = px(600.);

    with_view(&view, cx, |calculator, _, cx| {
        assert_eq!(calculator.results_width(window_width), px(200.));
        calculator.start_divider_drag(px(400.), window_width, cx);
        calculator.drag_divider(px(350.), window_width, cx);
        calculator.end_divider_drag(cx);
    });

    view.read_with(cx, |calculator, _| {
        assert_eq!(calculator.results_width(window_width), px(250.));
    });
    assert_eq!(
        SettingsFile::in_dir(dir.path()).load().result_pane_width,
        250.
    );
}
