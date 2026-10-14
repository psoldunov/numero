mod common;

use common::{Setup, id};
use numero_library::CalculationId;

#[test]
fn a_new_calculation_is_named_after_its_first_line() {
    let setup = Setup::new();
    let mut library = setup.open();

    let rome = library.create("\n  # Trip to Rome!\n€300 in USD").unwrap();
    let slash = library.create("a/b: c*d? <e> \"f\" | g\\h").unwrap();
    let kept = library
        .create("Rent - Q1 (2026), Tom & Jerry's_1.5")
        .unwrap();
    let dots = library.create("...hidden...\n").unwrap();
    let spaces = library.create("lots\t of    space").unwrap();

    assert_eq!(rome.as_str(), "Trip to Rome.txt");
    assert_eq!(slash.as_str(), "ab cd e f gh.txt");
    assert_eq!(kept.as_str(), "Rent - Q1 (2026), Tom & Jerry's_1.5.txt");
    assert_eq!(dots.as_str(), "hidden.txt");
    assert_eq!(spaces.as_str(), "lots of space.txt");
    assert_eq!(
        setup.read("Trip to Rome.txt"),
        "\n  # Trip to Rome!\n€300 in USD"
    );
}

#[test]
fn a_calculation_without_a_usable_line_is_untitled() {
    let setup = Setup::new();
    let mut library = setup.open();

    assert_eq!(library.create("").unwrap().as_str(), "Untitled.txt");
    assert_eq!(
        library.create("  ###  \n").unwrap().as_str(),
        "Untitled 2.txt"
    );
    assert_eq!(library.create("+*/").unwrap().as_str(), "Untitled 3.txt");
}

#[test]
fn long_first_lines_are_capped() {
    let setup = Setup::new();
    let mut library = setup.open();

    let created = library.create(&"ü".repeat(200)).unwrap();

    assert_eq!(created.stem().chars().count(), 60);
}

#[test]
fn names_are_de_duplicated_ignoring_case() {
    let setup = Setup::new();
    setup.write_external("rent.txt", "rent\n1");
    let mut library = setup.open();

    let second = library.create("Rent\n2").unwrap();
    let third = library.create("RENT\n3").unwrap();

    assert_eq!(second.as_str(), "Rent 2.txt");
    assert_eq!(third.as_str(), "RENT 3.txt");
    assert_eq!(setup.read("rent.txt"), "rent\n1");
}

#[test]
fn editing_never_renames() {
    let setup = Setup::new();
    let mut library = setup.open();
    let created = library.create("Groceries").unwrap();

    library.save(&created, "Something else entirely").unwrap();

    assert_eq!(setup.listing(), vec!["Groceries.txt"]);
    assert_eq!(setup.read("Groceries.txt"), "Something else entirely");
}

#[test]
fn ids_must_be_plain_visible_text_file_names() {
    for bad in [
        "",
        ".txt",
        ".hidden.txt",
        "notes.md",
        "notes",
        "../escape.txt",
        "a/b.txt",
        "a\\b.txt",
        "nul\0.txt",
        &format!("{}.txt", "a".repeat(252)),
    ] {
        assert!(
            CalculationId::try_from(bad).is_err(),
            "{bad:?} was accepted"
        );
    }
    assert_eq!(id("Shouting.TXT").stem(), "Shouting");
    assert_eq!(id(&format!("{}.txt", "a".repeat(251))).as_str().len(), 255);
    assert!(serde_json::from_str::<CalculationId>("\"../../etc/passwd\"").is_err());
    assert_eq!(
        serde_json::from_str::<CalculationId>("\"Rent.txt\"").unwrap(),
        id("Rent.txt")
    );
}
