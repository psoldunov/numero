//! The calculations folder as other devices change it, and choosing
//! another folder.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::{kept_in, library_in, open, text, type_document, with_view};
use crate::calculator::Calculator;
use crate::library::CalculationId;
use crate::settings::SettingsFile;

fn current(view: &Entity<Calculator>, cx: &mut VisualTestContext) -> CalculationId {
    view.read_with(cx, |calculator, _| calculator.current.clone().unwrap())
}

fn path_of(dir: &Path, id: &CalculationId) -> PathBuf {
    dir.join("calculations").join(id.as_str())
}

fn rescan(view: &Entity<Calculator>, cx: &mut VisualTestContext) {
    with_view(view, cx, Calculator::rescan);
}

/// Lets the debounced save run.
fn let_save(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
}

#[gpui::test]
fn calculations_are_plain_text_files_named_by_their_first_line(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));

    let id = current(&view, cx);
    assert_eq!(id.as_str(), "Sample.txt");
    type_document(&view, "# Sample\n2 + 2", cx);
    let_save(cx);
    assert_eq!(
        std::fs::read_to_string(path_of(dir.path(), &id)).unwrap(),
        "# Sample\n2 + 2"
    );
}

#[gpui::test]
fn a_change_from_another_device_shows_when_nothing_is_unsaved(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let id = current(&view, cx);

    std::fs::write(path_of(dir.path(), &id), "# Sample\n40 + 2").unwrap();
    rescan(&view, cx);

    assert_eq!(text(&view, cx), "# Sample\n40 + 2");
    let rows = view.read_with(cx, |calculator, _| calculator.rows.clone());
    assert_eq!(rows[0].display, "42");
}

#[gpui::test]
fn unsaved_edits_go_to_a_conflict_copy_instead_of_over_a_synced_change(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));
    let id = current(&view, cx);

    type_document(&view, "# Sample\nmine", cx);
    std::fs::write(path_of(dir.path(), &id), "# Sample\ntheirs").unwrap();
    rescan(&view, cx);
    // The edits stay on screen.
    assert_eq!(text(&view, cx), "# Sample\nmine");

    let_save(cx);

    let copy = current(&view, cx);
    assert_ne!(copy, id);
    assert!(copy.as_str().contains("(conflict)"), "{copy}");
    assert_eq!(
        std::fs::read_to_string(path_of(dir.path(), &id)).unwrap(),
        "# Sample\ntheirs"
    );
    assert_eq!(
        std::fs::read_to_string(path_of(dir.path(), &copy)).unwrap(),
        "# Sample\nmine"
    );
    assert_eq!(text(&view, cx), "# Sample\nmine");
}

#[gpui::test]
fn a_calculation_added_elsewhere_joins_the_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (view, cx) = open(cx, kept_in(dir.path()));

    std::fs::write(dir.path().join("calculations/Rent.txt"), "# Rent\n1200").unwrap();
    with_view(&view, cx, |calculator, window, cx| {
        calculator.set_list_open(true, window, cx);
        calculator.rescan(window, cx);
    });

    let titles: Vec<_> = view.read_with(cx, |calculator, _| {
        calculator
            .entries
            .iter()
            .map(|entry| entry.title.clone())
            .collect()
    });
    assert!(titles.contains(&Some("# Rent".to_owned())), "{titles:?}");
}

#[gpui::test]
fn another_folder_starts_empty_and_offers_to_move_the_calculations(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let synced = dir.path().join("Dropbox/Numero");
    let (view, cx) = open(cx, kept_in(dir.path()));

    with_view(&view, cx, |calculator, _, cx| {
        calculator
            .set_calculations_dir(Some(synced.clone()), cx)
            .unwrap();
    });
    assert_eq!(text(&view, cx), "");
    view.read_with(cx, |calculator, _| {
        assert_eq!(calculator.calculations_folder(), Some(synced.clone()));
        assert_eq!(calculator.move_offer, Some(dir.path().join("calculations")));
    });
    assert_eq!(
        SettingsFile::in_dir(dir.path()).load().calculations_dir,
        Some(synced.clone())
    );

    let moved = view.update(cx, |calculator, cx| calculator.move_calculations_here(cx));
    cx.run_until_parked();

    assert_eq!(moved, Ok(1));
    assert!(synced.join("Sample.txt").exists());
    assert!(library_in(dir.path()).is_empty());
    assert!(text(&view, cx).starts_with("# Sample"));
}
