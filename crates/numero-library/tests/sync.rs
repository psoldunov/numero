mod common;

use std::fs;
use std::sync::mpsc;
use std::time::Duration;

use common::{Setup, id, ids};
use numero_library::{Changes, Saved, watch};

#[test]
fn the_librarys_own_writes_are_never_reported() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    library.save(&rent, "Rent\n1200").unwrap();
    library.create("Food").unwrap();
    library.delete(&id("Food.txt")).unwrap();

    assert_eq!(library.rescan(), Changes::default());
    assert!(library.rescan().is_empty());
}

#[test]
fn rescan_picks_up_external_adds_modifications_and_deletions() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    let food = library.create("Food").unwrap();

    setup.write_external("From the Mac.txt", "From the Mac\n5 + 5");
    setup.write_external("Rent.txt", "Rent\n1300");
    fs::remove_file(setup.folder().join("Food.txt")).unwrap();
    let changes = library.rescan();

    assert_eq!(changes.added, vec![id("From the Mac.txt")]);
    assert_eq!(changes.removed, vec![food]);
    assert_eq!(changes.modified, vec![rent.clone()]);
    assert_eq!(ids(&library), vec!["From the Mac.txt", "Rent.txt"]);
    assert_eq!(ids(&setup.open()), vec!["From the Mac.txt", "Rent.txt"]);
}

#[test]
fn an_external_change_is_reported_once_until_it_changes_again() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();

    setup.write_external("Rent.txt", "Rent\n1300");
    assert_eq!(library.rescan().modified, vec![rent.clone()]);
    assert!(library.rescan().is_empty());

    setup.write_external("Rent.txt", "Rent\n1400");
    assert_eq!(library.rescan().modified, vec![rent.clone()]);

    assert_eq!(library.load(&rent).unwrap(), "Rent\n1400");
    assert!(library.rescan().is_empty());
}

#[test]
fn saving_after_an_external_change_writes_a_conflict_copy() {
    let setup = Setup::new();
    let mut library = setup.open();
    library.create("Older").unwrap();
    let rent = library.create("Rent").unwrap();
    library.create("Newer").unwrap();
    setup.write_external("Rent.txt", "Rent\nfrom the Mac");

    let saved = library.save(&rent, "Rent\nmine").unwrap();

    let copy = id("Rent (conflict).txt");
    assert_eq!(saved, Saved::Conflict { copy: copy.clone() });
    assert_eq!(setup.read("Rent.txt"), "Rent\nfrom the Mac");
    assert_eq!(setup.read("Rent (conflict).txt"), "Rent\nmine");
    assert_eq!(
        ids(&library),
        vec!["Newer.txt", "Rent (conflict).txt", "Rent.txt", "Older.txt"]
    );
    assert_eq!(setup.open().open_last("").unwrap().0, copy);
    assert_eq!(library.rescan().modified, vec![rent.clone()]);

    // Still unaccepted: a second save conflicts again, under a new name.
    let again = library.save(&rent, "Rent\nmine again").unwrap();
    assert_eq!(
        again,
        Saved::Conflict {
            copy: id("Rent (conflict 2).txt")
        }
    );

    // Once loaded, the change is accepted and saves go through.
    library.load(&rent).unwrap();
    assert_eq!(library.save(&rent, "Rent\nmerged").unwrap(), Saved::Written);
    assert_eq!(setup.read("Rent.txt"), "Rent\nmerged");
}

#[test]
fn saving_what_is_already_on_disk_is_harmless() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    setup.write_external("Rent.txt", "Rent\nsame");

    assert_eq!(library.save(&rent, "Rent\nsame").unwrap(), Saved::Written);
    assert!(library.rescan().is_empty());
    assert_eq!(setup.listing(), vec!["Rent.txt"]);
}

#[test]
fn saving_a_file_deleted_on_disk_writes_it_back() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    fs::remove_file(setup.folder().join("Rent.txt")).unwrap();
    assert_eq!(library.rescan().removed, vec![rent.clone()]);

    assert_eq!(library.save(&rent, "Rent\nback").unwrap(), Saved::Written);

    assert_eq!(setup.read("Rent.txt"), "Rent\nback");
    assert_eq!(ids(&library), vec!["Rent.txt"]);
    assert!(library.rescan().is_empty());
}

#[test]
fn nothing_but_calculations_is_ever_written_into_the_folder() {
    let setup = Setup::new();
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    library.save(&rent, "Rent\n1").unwrap();
    library.set_last_open(&rent);
    setup.write_external("Rent.txt", "changed");
    library.save(&rent, "conflicting").unwrap();
    library.rescan();

    assert_eq!(setup.listing(), vec!["Rent (conflict).txt", "Rent.txt"]);
}

#[test]
fn the_watcher_reports_an_external_write() {
    let setup = Setup::new();
    let library = setup.open();
    let (sender, receiver) = mpsc::channel();
    let _watcher = watch(library.folder(), move || {
        let _ = sender.send(());
    })
    .unwrap();

    setup.write_external("From the Mac.txt", "hello");

    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("no change reported within 5 s");
}
