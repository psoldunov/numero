mod common;

use std::fs;

use common::{Setup, id, ids};
use numero_library::{Library, title};

const WELCOME: &str = "# Sample\n8 / (45 - 20%)";

#[test]
fn the_first_run_opens_a_new_calculation_with_the_welcome() {
    let setup = Setup::new();
    let mut library = setup.open();
    assert!(library.is_empty());

    let (created, text) = library.open_last(WELCOME).unwrap();

    assert_eq!(created.as_str(), "Sample.txt");
    assert_eq!(text, WELCOME);
    let reopened = setup.open().open_last(WELCOME).unwrap();
    assert_eq!(reopened, (created, WELCOME.to_owned()));
}

#[test]
fn the_calculation_open_last_reopens_at_start() {
    let setup = Setup::new();
    let mut library = setup.open();
    let first = library.create("one\n1 + 1").unwrap();
    library.create("two\n2 + 2").unwrap();
    library.set_last_open(&first);

    let (opened, text) = setup.open().open_last(WELCOME).unwrap();

    assert_eq!((opened, text.as_str()), (first, "one\n1 + 1"));
}

#[test]
fn without_a_last_open_the_newest_opens() {
    let setup = Setup::new();
    let mut library = setup.open();
    let first = library.create("one").unwrap();
    library.create("two").unwrap();
    library.delete(&id("two.txt")).unwrap();

    let (opened, _) = setup.open().open_last(WELCOME).unwrap();

    assert_eq!(opened, first);
}

#[test]
fn new_calculations_come_first_and_list_their_first_line() {
    let setup = Setup::new();
    let mut library = setup.open();
    library.create("\n  # Rent\n1200").unwrap();
    library.create("").unwrap();

    let entries = library.entries();

    assert_eq!(ids(&library), vec!["Untitled.txt", "Rent.txt"]);
    assert_eq!(entries[0].title, None);
    assert_eq!(entries[1].title.as_deref(), Some("# Rent"));
    assert_eq!(title("\n \n  x  \ny"), Some("x".to_owned()));
}

#[test]
fn delete_removes_the_file_and_its_place() {
    let setup = Setup::new();
    let mut library = setup.open();
    let gone = library.create("gone").unwrap();
    library.create("kept").unwrap();
    library.set_last_open(&gone);

    library.delete(&gone).unwrap();
    library.delete(&gone).unwrap();

    assert_eq!(ids(&library), vec!["kept.txt"]);
    assert_eq!(setup.listing(), vec!["kept.txt"]);
    assert_eq!(setup.open().open_last(WELCOME).unwrap().0, id("kept.txt"));
}

#[test]
fn search_ignores_case_and_an_empty_query_matches_all() {
    let setup = Setup::new();
    let mut library = setup.open();
    library.create("Rent\n1200 EUR").unwrap();
    library.create("Groceries\n$40").unwrap();

    let matching = |query: &str| -> Vec<String> {
        library
            .entries()
            .into_iter()
            .filter(|entry| entry.matches(query))
            .map(|entry| entry.id.to_string())
            .collect()
    };

    assert_eq!(matching("eur"), vec!["Rent.txt"]);
    assert_eq!(matching(" GROC "), vec!["Groceries.txt"]);
    assert_eq!(matching(""), vec!["Groceries.txt", "Rent.txt"]);
}

#[test]
fn hidden_files_other_files_and_folders_are_ignored() {
    let setup = Setup::new();
    setup.write_external(".Rent.txt.tmp", "half written");
    setup.write_external(".hidden.txt", "hidden");
    setup.write_external("notes.md", "markdown");
    setup.write_external("Real.txt", "real");
    fs::create_dir(setup.folder().join("Folder.txt")).unwrap();

    let library = setup.open();

    assert_eq!(ids(&library), vec!["Real.txt"]);
}

#[test]
fn unknown_files_go_on_top_newest_first_and_missing_ones_are_dropped() {
    let setup = Setup::new();
    let mut library = setup.open();
    library.create("known").unwrap();
    library.create("gone").unwrap();
    fs::remove_file(setup.folder().join("gone.txt")).unwrap();
    setup.write_external("older.txt", "older");
    set_modified(&setup, "older.txt", 1_000);
    setup.write_external("newer.txt", "newer");
    set_modified(&setup, "newer.txt", 2_000);

    let library = setup.open();

    assert_eq!(ids(&library), vec!["newer.txt", "older.txt", "known.txt"]);
}

#[test]
fn the_metadata_lives_outside_the_folder_keyed_by_folder() {
    let setup = Setup::new();
    let other = setup.folder().with_file_name("Other");
    let mut library = setup.open();
    library.create("a").unwrap();
    library.create("b").unwrap();
    library.set_last_open(&id("a.txt"));
    let mut elsewhere = Library::open(&other, &setup.state());
    elsewhere.create("z").unwrap();

    assert_eq!(setup.listing(), vec!["a.txt", "b.txt"]);
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(setup.state().join("library.json")).unwrap())
            .unwrap();
    let folders = json["folders"].as_object().unwrap();
    assert_eq!(folders.len(), 2);
    let key = fs::canonicalize(setup.folder()).unwrap();
    let ours = &folders[key.to_str().unwrap()];
    assert_eq!(ours["order"], serde_json::json!(["b.txt", "a.txt"]));
    assert_eq!(ours["last_open"], "a.txt");
    assert_eq!(ids(&setup.open()), vec!["b.txt", "a.txt"]);
    assert_eq!(ids(&Library::open(&other, &setup.state())), vec!["z.txt"]);
}

#[test]
fn a_hand_edited_metadata_file_cannot_name_files_outside_the_folder() {
    let setup = Setup::new();
    setup.open().create("safe").unwrap();
    fs::write(setup.folder().with_file_name("secret.txt"), "secret").unwrap();
    let key = fs::canonicalize(setup.folder()).unwrap();
    let json = serde_json::json!({
        "version": 1,
        "folders": { key.to_str().unwrap(): {
            "order": ["../secret.txt", "safe.txt"],
            "last_open": "../secret.txt"
        }}
    });
    fs::write(setup.state().join("library.json"), json.to_string()).unwrap();

    let mut library = setup.open();

    assert_eq!(ids(&library), vec!["safe.txt"]);
    assert_eq!(library.open_last(WELCOME).unwrap().0, id("safe.txt"));
}

#[allow(clippy::unwrap_used)]
fn set_modified(setup: &Setup, name: &str, seconds: u64) {
    let file = fs::File::options()
        .write(true)
        .open(setup.folder().join(name))
        .unwrap();
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    file.set_modified(time).unwrap();
}
