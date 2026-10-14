use std::fs;

use super::*;

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open_in(dir.path());
    (dir, library)
}

#[test]
fn the_first_run_opens_a_new_calculation_with_the_sample() {
    let (dir, mut library) = library();

    let (id, text) = library.open_last().unwrap();

    assert_eq!(text, SAMPLE);
    assert_eq!(library.load(&id).unwrap(), SAMPLE);
    let reopened = Library::open_in(dir.path()).open_last().unwrap();
    assert_eq!(reopened, (id, SAMPLE.to_owned()));
}

#[test]
fn the_calculation_open_last_reopens_at_start() {
    let (dir, mut library) = library();
    let first = library.create("1 + 1").unwrap();
    library.create("2 + 2").unwrap();
    library.set_last_open(&first);

    let (id, text) = Library::open_in(dir.path()).open_last().unwrap();

    assert_eq!((id, text.as_str()), (first, "1 + 1"));
}

#[test]
fn new_calculations_come_first_and_list_their_first_line() {
    let (_dir, mut library) = library();
    library.create("\n  # Rent\n1200").unwrap();
    library.create("").unwrap();

    let entries = library.entries();

    let titles: Vec<_> = entries.iter().map(|entry| entry.title.clone()).collect();
    assert_eq!(titles, [None, Some("# Rent".to_owned())]);
}

#[test]
fn a_search_matches_anywhere_in_the_text_ignoring_case() {
    let entry = Entry {
        id: CalculationId::new(1),
        title: title("# Trip\n120 EUR in USD"),
        text: "# Trip\n120 EUR in USD".to_owned(),
    };
    assert!(entry.matches("eur"));
    assert!(entry.matches("  "));
    assert!(!entry.matches("rent"));
}

#[test]
fn a_deleted_calculation_is_gone_from_disk_and_the_list() {
    let (dir, mut library) = library();
    let kept = library.create("1").unwrap();
    let gone = library.create("2").unwrap();

    library.delete(&gone).unwrap();

    assert!(library.load(&gone).is_err());
    let ids: Vec<_> = Library::open_in(dir.path())
        .entries()
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert_eq!(ids, [kept]);
}

#[test]
fn files_the_index_lost_are_listed_and_missing_ones_dropped() {
    let (dir, mut library) = library();
    let id = library.create("3 * 3").unwrap();
    fs::write(dir.path().join(INDEX), "not json").unwrap();
    fs::write(dir.path().join("abc.numi"), "7").unwrap();

    let entries = Library::open_in(dir.path()).entries();

    let ids: Vec<_> = entries.into_iter().map(|entry| entry.id).collect();
    assert!(ids.contains(&id));
    assert!(ids.contains(&CalculationId("abc".to_owned())));
}

#[test]
fn an_index_naming_a_path_outside_the_directory_is_ignored() {
    let (dir, _library) = library();
    fs::write(
        dir.path().join(INDEX),
        r#"{"order": ["../../etc/passwd"], "last_open": "../x"}"#,
    )
    .unwrap();

    let mut library = Library::open_in(dir.path());

    let (_, text) = library.open_last().unwrap();
    assert_eq!(text, SAMPLE);
}
