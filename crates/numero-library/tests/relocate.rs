mod common;

use std::fs;

use common::{Setup, id, ids, listing};
use numero_library::{Library, Saved};

#[test]
fn move_into_carries_files_order_and_last_open_over() {
    let setup = Setup::new();
    let target = setup.folder().with_file_name("iCloud").join("Numero");
    let mut library = setup.open();
    let rent = library.create("Rent").unwrap();
    library.create("Food").unwrap();
    library.set_last_open(&rent);

    let mut moved = library.move_into(&target).unwrap();

    assert_eq!(moved.folder(), target);
    assert_eq!(setup.listing(), Vec::<String>::new());
    assert_eq!(listing(&target), vec!["Food.txt", "Rent.txt"]);
    assert_eq!(ids(&moved), vec!["Food.txt", "Rent.txt"]);
    assert_eq!(moved.open_last("").unwrap().0, rent);
    let reopened = Library::open(&target, &setup.state());
    assert_eq!(ids(&reopened), vec!["Food.txt", "Rent.txt"]);
    // What was known moved too: a save is not a conflict.
    assert_eq!(moved.save(&rent, "Rent\n2").unwrap(), Saved::Written);
}

#[test]
fn move_into_never_overwrites_and_lists_moved_files_first() {
    let setup = Setup::new();
    let target = setup.folder().with_file_name("Shared");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("rent.txt"), "theirs").unwrap();
    let mut library = setup.open();
    library.create("Rent\nmine").unwrap();

    let moved = library.move_into(&target).unwrap();

    assert_eq!(
        fs::read_to_string(target.join("rent.txt")).unwrap(),
        "theirs"
    );
    assert_eq!(
        fs::read_to_string(target.join("Rent 2.txt")).unwrap(),
        "Rent\nmine"
    );
    assert_eq!(ids(&moved), vec!["Rent 2.txt", "rent.txt"]);
    assert_eq!(
        moved.entries().first().map(|entry| entry.id.clone()),
        Some(id("Rent 2.txt"))
    );
}

#[test]
fn moving_into_the_same_folder_changes_nothing() {
    let setup = Setup::new();
    let mut library = setup.open();
    library.create("Rent").unwrap();

    let same = library.move_into(&setup.folder()).unwrap();

    assert_eq!(ids(&same), vec!["Rent.txt"]);
    assert_eq!(setup.listing(), vec!["Rent.txt"]);
}
