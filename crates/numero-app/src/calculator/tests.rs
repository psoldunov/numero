use std::path::Path;
use std::time::Duration;

use gpui::{
    Entity, EntityInputHandler as _, Modifiers, TestAppContext, VisualTestContext, point, px,
};

use gpui_component::input::InputEvent;

use super::{Calculator, Sources, Zoom};
use crate::app::{ZoomIn, ZoomOut};
use crate::library::{Library, SAMPLE};
use crate::settings::{DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE, SettingsFile};

/// Sources kept under `dir`, as the app keeps them under the user's.
fn kept_in(dir: &Path) -> Sources {
    Sources {
        library: Some(Library::open_in(&dir.join("calculations"))),
        settings: Some(SettingsFile::in_dir(dir)),
    }
}

fn open(cx: &mut TestAppContext, sources: Sources) -> (Entity<Calculator>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let opened = cx.add_window_view(|window, cx| Calculator::new(sources, window, cx));
    let (_, window) = &opened;
    window.run_until_parked();
    opened
}

/// Replaces the whole calculation as typing would, so the editor reports
/// it.
fn type_document(view: &Entity<Calculator>, text: &str, cx: &mut VisualTestContext) {
    let editor = view.read_with(cx, |calculator, _| calculator.editor.clone());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.select_all(window, cx);
            editor.replace_text_in_range(None, text, window, cx);
        });
    });
    cx.run_until_parked();
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn text(view: &Entity<Calculator>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |calculator, cx| calculator.text(cx))
}

fn with_view(
    view: &Entity<Calculator>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut Calculator, &mut gpui::Window, &mut gpui::Context<'_, Calculator>),
) {
    cx.update(|window, cx| view.update(cx, |calculator, cx| f(calculator, window, cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn the_first_run_opens_the_sample_with_a_result_per_line(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));

    assert_eq!(text(&view, cx), SAMPLE);
    view.read_with(cx, |calculator, _| {
        assert_eq!(calculator.results.len(), SAMPLE.lines().count());
    });
}

#[gpui::test]
fn an_edit_evaluates_the_calculation_and_colours_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());

    type_document(&view, "# Total\n42\n", cx);

    view.read_with(cx, |calculator, cx| {
        assert_eq!(calculator.results.len(), 3);
        let lines: Vec<usize> = calculator.rows.iter().map(|row| row.line).collect();
        assert_eq!(lines, [1]);
        assert!(!calculator.rows[0].display.is_empty());
        assert!(!calculator.highlights.get_ranges(cx).is_empty());
    });
}

#[gpui::test]
fn an_edit_is_saved_once_typing_pauses_and_reopens_next_time(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let id = view.read_with(cx, |calculator, _| calculator.current.clone().unwrap());
    let library = Library::open_in(&dir.path().join("calculations"));

    type_document(&view, "1 + 1", cx);
    assert_eq!(library.load(&id).unwrap(), SAMPLE);

    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_eq!(library.load(&id).unwrap(), "1 + 1");
    let (reopened, _) = Library::open_in(&dir.path().join("calculations"))
        .open_last()
        .unwrap();
    assert_eq!(reopened, id);
}

#[gpui::test]
fn a_new_calculation_starts_empty_and_the_old_one_is_kept(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let first = view.read_with(cx, |calculator, _| calculator.current.clone().unwrap());
    type_document(&view, "2 * 21", cx);

    with_view(&view, cx, Calculator::new_calculation);

    assert_eq!(text(&view, cx), "");
    let second = view.read_with(cx, |calculator, _| calculator.current.clone().unwrap());
    assert_ne!(first, second);
    // The edit made just before was saved on the way out, unwaited.
    let library = Library::open_in(&dir.path().join("calculations"));
    assert_eq!(library.load(&first).unwrap(), "2 * 21");

    with_view(&view, cx, |calculator, window, cx| {
        calculator.open_calculation(&first, window, cx);
    });
    assert_eq!(text(&view, cx), "2 * 21");
}

#[gpui::test]
fn the_list_filters_as_you_type_and_enter_opens_the_first_match(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    type_document(&view, "# Rent\n1200", cx);
    with_view(&view, cx, Calculator::new_calculation);
    type_document(&view, "# Trip\n120 EUR in USD", cx);

    with_view(&view, cx, |calculator, window, cx| {
        calculator.set_list_open(true, window, cx);
    });
    let search = view.read_with(cx, |calculator, _| calculator.search.clone());
    cx.update(|window, cx| {
        search.update(cx, |search, cx| {
            search.replace_text_in_range(None, "rent", window, cx)
        });
    });
    cx.run_until_parked();

    let titles: Vec<_> = view.read_with(cx, |calculator, cx| {
        calculator
            .visible_entries(cx)
            .into_iter()
            .map(|entry| entry.title)
            .collect()
    });
    assert_eq!(titles, [Some("# Rent".to_owned())]);

    // The field reports Enter as `PressEnter`; the popover it sits in needs
    // gpui-component's `Root`, which a test window doesn't have.
    search.update(cx, |_, cx| {
        cx.emit(InputEvent::PressEnter {
            secondary: false,
            shift: false,
        });
    });
    cx.run_until_parked();
    assert_eq!(text(&view, cx), "# Rent\n1200");
    assert!(!view.read_with(cx, |calculator, _| calculator.list_open));
}

#[gpui::test]
fn deleting_asks_once_then_removes_the_calculation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let first = view.read_with(cx, |calculator, _| calculator.current.clone().unwrap());
    with_view(&view, cx, Calculator::new_calculation);

    with_view(&view, cx, |calculator, window, cx| {
        calculator.delete_pressed(&first, window, cx);
    });
    let library = Library::open_in(&dir.path().join("calculations"));
    assert!(library.load(&first).is_ok(), "the first press only asks");

    with_view(&view, cx, |calculator, window, cx| {
        calculator.delete_pressed(&first, window, cx);
    });
    assert!(library.load(&first).is_err());
}

#[gpui::test]
fn deleting_the_open_calculation_opens_the_next(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    type_document(&view, "7", cx);
    with_view(&view, cx, Calculator::new_calculation);
    let second = view.read_with(cx, |calculator, _| calculator.current.clone().unwrap());

    with_view(&view, cx, |calculator, window, cx| {
        calculator.delete_pressed(&second, window, cx);
        calculator.delete_pressed(&second, window, cx);
    });

    assert_eq!(text(&view, cx), "7");
}

#[gpui::test]
fn settings_apply_at_once_and_are_kept(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    type_document(&view, "1/3", cx);
    let before = view.read_with(cx, |calculator, _| calculator.rows[0].display.clone());

    with_view(&view, cx, |calculator, window, cx| {
        calculator.update_settings(|settings| settings.precision = 5, window, cx);
    });

    let after = view.read_with(cx, |calculator, _| calculator.rows[0].display.clone());
    assert_ne!(before, after);
    assert_eq!(SettingsFile::in_dir(dir.path()).load().precision, 5);
}

#[gpui::test]
fn a_click_copies_the_result_and_says_so_for_a_moment(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    type_document(&view, "42", cx);
    let display = view.read_with(cx, |calculator, _| calculator.rows[0].display.clone());

    view.update(cx, |calculator, cx| calculator.copy_result(0, cx));

    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some(display.as_str()));
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.copied),
        Some(0)
    );

    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |calculator, _| calculator.copied), None);
}

#[gpui::test]
fn copy_with_results_puts_each_result_beside_its_line(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    type_document(&view, "# Two\n1 + 1", cx);

    view.update(cx, |calculator, cx| calculator.copy_with_results(cx));

    let copied = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap();
    let mut lines = copied.lines();
    assert_eq!(lines.next(), Some("# Two"));
    assert!(lines.next().unwrap().starts_with("1 + 1    "));
}

#[gpui::test]
fn the_zoom_shortcuts_resize_the_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    draw(cx);

    cx.dispatch_action(ZoomIn);
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.text_size()),
        px(DEFAULT_FONT_SIZE + 2.)
    );
    cx.dispatch_action(ZoomOut);
    cx.dispatch_action(ZoomOut);
    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.text_size()),
        px(DEFAULT_FONT_SIZE - 2.)
    );
}

#[test]
fn zooming_stays_within_its_limits() {
    assert_eq!(Zoom::In.apply(MAX_FONT_SIZE), MAX_FONT_SIZE);
    assert_eq!(Zoom::Out.apply(MIN_FONT_SIZE), MIN_FONT_SIZE);
    assert_eq!(Zoom::Reset.apply(30.), DEFAULT_FONT_SIZE);
}

#[gpui::test]
fn a_result_stays_level_with_its_line_when_the_editor_scrolls(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, Sources::default());
    let text: Vec<String> = (1..=80).map(|n| n.to_string()).collect();
    type_document(&view, &text.join("\n"), cx);
    draw(cx);
    let editor = view.read_with(cx, |calculator, _| calculator.editor.clone());
    let line_height = view.read_with(cx, |calculator, cx| calculator.line_height(cx));

    // Scroll 20 lines down, then let the editor settle on it.
    editor.update(cx, |editor, cx| {
        editor.set_scroll_offset(point(px(0.), -line_height * 20.), cx);
    });
    draw(cx);
    draw(cx);
    assert_eq!(
        editor.read_with(cx, |editor, _| editor.scroll_offset().y),
        -line_height * 20.
    );

    // Click where line 25's result should be: level with the line, at the
    // window's right edge.
    let line = 24;
    let start: usize = text[..line].iter().map(|line| line.len() + 1).sum();
    let line_bounds = editor
        .read_with(cx, |editor, _| editor.range_to_bounds(&(start..start)))
        .expect("line 25 is on screen");
    let width = cx.update(|window, _| window.viewport_size().width);
    cx.simulate_click(
        point(width - px(22.), line_bounds.center().y),
        Modifiers::none(),
    );

    assert_eq!(
        view.read_with(cx, |calculator, _| calculator.copied),
        Some(line)
    );
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("25"));
}
