//! Loading extensions from a temporary folder into an engine.

use std::path::Path;
use std::time::{Duration, Instant};

use numero_engine::Engine;
use numero_extensions::{DEFAULT_FILE, DEFAULT_SOURCE, LoadLog, load};

fn folder(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("folders");
        }
        std::fs::write(path, source).expect("write");
    }
    dir
}

fn loaded(files: &[(&str, &str)]) -> (Engine, LoadLog, tempfile::TempDir) {
    let dir = folder(files);
    let mut engine = Engine::default();
    let log = load(dir.path(), &mut engine);
    (engine, log, dir)
}

fn result(engine: &Engine, line: &str) -> Option<String> {
    engine.evaluate(line).pop().and_then(|r| r.display)
}

fn loading_lines(log: &LoadLog) -> Vec<&str> {
    log.lines
        .iter()
        .filter_map(|l| l.strip_prefix("Loading "))
        .collect()
}

#[test]
fn seeds_default_js_into_a_missing_folder() {
    let root = tempfile::tempdir().expect("temp dir");
    let dir = root.path().join("numero").join("extensions");
    let mut engine = Engine::default();
    let log = load(&dir, &mut engine);
    let seeded = std::fs::read_to_string(dir.join(DEFAULT_FILE)).expect("seeded");
    assert_eq!(seeded, DEFAULT_SOURCE);
    assert!(seeded.contains("restart Numero"));
    assert_eq!(loading_lines(&log), ["Default.js"]);
    assert_eq!(log.errors().count(), 0, "{log:?}");
    // The examples are commented out, so nothing is defined: `zum` is free
    // text and, as in Numi, only the first value of `(2;3)` is kept.
    assert_ne!(result(&engine, "zum(2;3)").as_deref(), Some("5"));
}

#[test]
fn keeps_an_existing_default_js() {
    let (_, _, dir) = loaded(&[("Default.js", "log('mine')")]);
    let kept = std::fs::read_to_string(dir.path().join(DEFAULT_FILE)).expect("read");
    assert_eq!(kept, "log('mine')");
}

#[test]
fn loads_every_js_file_recursively_in_sorted_order_in_one_context() {
    let (engine, log, _dir) = loaded(&[
        ("b.js", "shared.push('b'); log(shared.join(','));"),
        ("a.js", "var shared = ['a'];"),
        (
            "sub/deeper/c.js",
            "shared.push('c'); log(shared.join(','));",
        ),
        ("sub/a2.js", "shared.push('a2');"),
        ("notes.txt", "this is not javascript"),
        ("sub/readme.JSON", "{}"),
    ]);
    assert_eq!(
        loading_lines(&log),
        ["Default.js", "a.js", "b.js", "sub/a2.js", "sub/deeper/c.js"]
    );
    assert!(log.lines.contains(&"a,b".to_string()), "{log:?}");
    assert!(log.lines.contains(&"a,b,a2,c".to_string()), "{log:?}");
    assert_eq!(log.errors().count(), 0, "{log:?}");
    drop(engine);
}

#[test]
fn log_joins_its_arguments() {
    let (_, log, _dir) = loaded(&[("a.js", "log('x', 1, true); log();")]);
    let after = log
        .lines
        .iter()
        .skip_while(|l| *l != "Loading a.js")
        .skip(1)
        .collect::<Vec<_>>();
    assert_eq!(after, ["x 1 true", ""]);
}

#[test]
fn set_variable_with_numbers_and_value_dicts() {
    let (engine, log, _dir) = loaded(&[(
        "vars.js",
        r#"
        numi.setVariable("anothervar", 333);
        numi.setVariable("myvar", { "double": 5, "unitId": "USD" });
        numi.setVariable("len", { "decimal": "2", "unit": "km" });
        "#,
    )]);
    assert_eq!(log.errors().count(), 0, "{log:?}");
    assert_eq!(result(&engine, "anothervar + 1").as_deref(), Some("334"));
    assert_eq!(result(&engine, "myvar * 2").as_deref(), Some("$10"));
    assert_eq!(result(&engine, "len in m").as_deref(), Some("2,000 m"));
}

#[test]
fn engine_rejections_are_logged() {
    let (_, log, _dir) = loaded(&[(
        "bad.js",
        r#"
        numi.setVariable("x", { "double": 1, "unitId": "parsec" });
        numi.addUnit({ "id": "blob", "phrases": "blob", "baseUnitId": "USD", "format": "blob", "ratio": 1 });
        "#,
    )]);
    let errors: Vec<_> = log.errors().collect();
    assert_eq!(errors.len(), 2, "{log:?}");
    assert!(errors[0].contains("parsec"), "{errors:?}");
    assert!(errors[1].contains("blob"), "{errors:?}");
}

#[test]
fn add_unit_converts() {
    let (engine, log, _dir) = loaded(&[(
        "horse.js",
        r#"
        numi.addUnit({
            "id": "horse",
            "phrases": "horse, horses, hrs",
            "baseUnitId": "m",
            "format": "hrs",
            "ratio": 2.4,
            "internationalSymbol": "hrs",
        });
        "#,
    )]);
    assert_eq!(log.errors().count(), 0, "{log:?}");
    assert_eq!(result(&engine, "3 horses in m").as_deref(), Some("7.2 m"));
    assert_eq!(
        result(&engine, "10 m in horses").as_deref(),
        Some("4.17 hrs")
    );
}

#[test]
fn add_function_is_callable_from_a_document() {
    let (engine, log, _dir) = loaded(&[(
        "zum.js",
        r#"
        numi.addFunction({ "id": "zum", "phrases": "zum" }, function (values) {
            return { "double": values[0].double + values[1].double };
        });
        numi.addFunction({ "id": "twice", "phrases": "twice" }, function (values) {
            return values[0].double * 2;
        });
        numi.addFunction({ "id": "same", "phrases": "same" }, function (values) {
            const v = values[0];
            return { "double": v.double, "unitId": v.unitId };
        });
        numi.addFunction({ "id": "kind", "phrases": "kind" }, function (values) {
            return values.length * 100 + (values[0].type === "unit" ? 1 : 0);
        });
        "#,
    )]);
    assert_eq!(log.errors().count(), 0, "{log:?}");
    let lines = engine.evaluate("zum(2;3)\nzum(2; 3) * 2\ntwice(21)\nsame(3 km) in m\nkind(5 m)");
    let shown: Vec<_> = lines.into_iter().map(|r| r.display).collect();
    assert_eq!(
        shown,
        [
            Some("5".to_string()),
            Some("10".to_string()),
            Some("42".to_string()),
            Some("3,000 m".to_string()),
            Some("101".to_string()),
        ]
    );
}

#[test]
fn exceptions_in_functions_give_no_result() {
    let (engine, _, _dir) = loaded(&[(
        "throws.js",
        r#"
        numi.addFunction({ "id": "boom", "phrases": "boom" }, function () { throw new Error("no"); });
        numi.addFunction({ "id": "junk", "phrases": "junk" }, function () { return "text"; });
        "#,
    )]);
    assert_eq!(result(&engine, "boom(1)"), None);
    assert_eq!(result(&engine, "junk(1)"), None);
}

#[test]
fn exceptions_are_logged_with_line_numbers_and_later_files_still_load() {
    let (engine, log, _dir) = loaded(&[
        ("a.js", "\n\nundefinedThing();\n"),
        ("b.js", "var x = ;"),
        ("c.js", "\nnumi.addUnit({ \"id\": \"nope\" });"),
        ("d.js", "numi.setVariable('later', 7);"),
    ]);
    let errors: Vec<_> = log.errors().collect();
    assert_eq!(errors.len(), 3, "{log:?}");
    assert!(errors[0].starts_with("Error on line 3: "), "{errors:?}");
    assert!(errors[0].contains("undefinedThing"), "{errors:?}");
    assert!(errors[1].starts_with("Error on line 1: "), "{errors:?}");
    assert!(errors[2].starts_with("Error on line 2: "), "{errors:?}");
    assert!(errors[2].contains("baseUnitId"), "{errors:?}");
    assert_eq!(result(&engine, "later").as_deref(), Some("7"));
}

#[test]
fn a_runaway_load_is_stopped() {
    let (engine, log, _dir) = loaded(&[
        ("a.js", "while (true) {}"),
        ("b.js", "numi.setVariable('after', 1);"),
    ]);
    let errors: Vec<_> = log.errors().collect();
    assert_eq!(errors.len(), 1, "{log:?}");
    assert!(errors[0].contains("ran longer than"), "{errors:?}");
    assert_eq!(result(&engine, "after").as_deref(), Some("1"));
}

#[test]
fn a_timed_out_function_yields_no_result_rather_than_hanging() {
    let (engine, _, _dir) = loaded(&[(
        "slow.js",
        r#"
        numi.addFunction({ "id": "spin", "phrases": "spin" }, function () { while (true) {} });
        numi.addFunction({ "id": "zum", "phrases": "zum" }, function (v) { return v[0].double + v[1].double; });
        "#,
    )]);
    let started = Instant::now();
    let lines = engine.evaluate(&"spin(1)\n".repeat(20));
    assert!(lines.iter().all(|r| r.display.is_none()));
    // One timeout, then the cooldown answers the rest at once.
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    // The thread survived the interrupt.
    assert_eq!(result(&engine, "zum(2;3)").as_deref(), Some("5"));
}

#[test]
fn performance_budgets() {
    let source = r#"
        numi.addFunction({ "id": "zum", "phrases": "zum" }, function (v) {
            return { "double": v[0].double + v[1].double };
        });
        numi.addUnit({ "id": "horse", "phrases": "horse, horses", "baseUnitId": "m", "format": "hrs", "ratio": 2.4 });
        numi.setVariable("rate", 3);
    "#;
    let dir = folder(&[
        ("a.js", source),
        ("b.js", "log('b')"),
        ("c/d.js", "log('d')"),
    ]);
    let mut engine = Engine::default();
    let started = Instant::now();
    let log = load(dir.path(), &mut engine);
    let load_time = started.elapsed();
    assert_eq!(log.errors().count(), 0, "{log:?}");
    let document = (0..100)
        .map(|i| format!("zum({i}; 3)"))
        .collect::<Vec<_>>()
        .join("\n");
    // Warm up once, as the editor does on its first keystroke.
    let _ = engine.evaluate(&document);
    let started = Instant::now();
    let lines = engine.evaluate(&document);
    let eval_time = started.elapsed();
    assert_eq!(lines[99].display.as_deref(), Some("102"));
    let release = !cfg!(debug_assertions);
    eprintln!("load {load_time:?}, 100 calls {eval_time:?}");
    if release {
        assert!(load_time < Duration::from_millis(50), "{load_time:?}");
        assert!(eval_time < Duration::from_millis(20), "{eval_time:?}");
    } else {
        assert!(load_time < Duration::from_millis(500), "{load_time:?}");
        assert!(eval_time < Duration::from_millis(200), "{eval_time:?}");
    }
}

#[test]
fn engine_clones_share_the_functions() {
    let (engine, _, dir) = loaded(&[(
        "zum.js",
        "numi.addFunction({ id: 'zum', phrases: 'zum, add2' }, v => v[0].double + v[1].double);",
    )]);
    let clone = engine.clone();
    drop(engine);
    assert_eq!(result(&clone, "add2(1;1)").as_deref(), Some("2"));
    assert!(Path::new(&dir.path().join(DEFAULT_FILE)).exists());
}
