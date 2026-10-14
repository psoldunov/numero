mod common;

use std::fs;

use common::{Setup, id, ids, listing};

/// The first layout: `<hex id>.numi` files and an `index.json`.
#[allow(clippy::unwrap_used)]
fn legacy(setup: &Setup) {
    let folder = setup.folder();
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("18f0a.numi"), "# Rent\n1200").unwrap();
    fs::write(folder.join("18f0b.numi"), "Groceries\n$40").unwrap();
    fs::write(folder.join("18f0c.numi"), "rent\nagain").unwrap();
    fs::write(folder.join("18f0d.numi"), "not in the index").unwrap();
    let index = r#"{"order":["18f0b","18f0a","18f0c"],"last_open":"18f0a"}"#;
    fs::write(folder.join("index.json"), index).unwrap();
}

#[test]
fn a_legacy_folder_becomes_named_text_files_in_the_same_order() {
    let setup = Setup::new();
    legacy(&setup);

    let mut library = setup.open();

    assert_eq!(
        ids(&library),
        vec![
            "Groceries.txt",
            "Rent.txt",
            "rent 2.txt",
            "not in the index.txt"
        ]
    );
    assert_eq!(setup.read("Rent.txt"), "# Rent\n1200");
    assert_eq!(
        setup.listing(),
        vec![
            "Groceries.txt",
            "Rent.txt",
            "not in the index.txt",
            "rent 2.txt"
        ]
    );
    assert_eq!(library.open_last("").unwrap().0, id("Rent.txt"));
    assert_eq!(
        listing(&setup.state().join("legacy-backup")),
        vec![
            "18f0a.numi",
            "18f0b.numi",
            "18f0c.numi",
            "18f0d.numi",
            "index.json"
        ]
    );
}

#[test]
fn without_an_index_legacy_files_come_newest_id_first() {
    let setup = Setup::new();
    let folder = setup.folder();
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("ff.numi"), "older").unwrap();
    fs::write(folder.join("1a0.numi"), "newest").unwrap();
    fs::write(folder.join("fe.numi"), "oldest").unwrap();

    let library = setup.open();

    assert_eq!(ids(&library), vec!["newest.txt", "older.txt", "oldest.txt"]);
}

#[test]
fn migration_happens_once() {
    let setup = Setup::new();
    legacy(&setup);
    let mut first = setup.open();
    first.set_last_open(&id("Groceries.txt"));
    let backups_before = listing(&setup.state().join("legacy-backup"));

    let mut second = setup.open();

    assert_eq!(ids(&second), ids(&first));
    assert_eq!(second.open_last("").unwrap().0, id("Groceries.txt"));
    assert_eq!(
        listing(&setup.state().join("legacy-backup")),
        backups_before
    );
    assert_eq!(setup.listing().len(), 4);
}

#[test]
fn files_already_in_the_folder_are_kept_beside_migrated_ones() {
    let setup = Setup::new();
    legacy(&setup);
    setup.write_external("Groceries.txt", "from Numi");

    let library = setup.open();

    assert_eq!(setup.read("Groceries.txt"), "from Numi");
    assert_eq!(setup.read("Groceries 2.txt"), "Groceries\n$40");
    assert_eq!(library.entries().len(), 5);
}
