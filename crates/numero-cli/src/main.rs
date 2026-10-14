//! `numero [FILE]`: evaluate a document (a file, or stdin) and print each
//! line beside its result.

use std::io::Read;

fn main() -> std::io::Result<()> {
    let mut document = String::new();
    match std::env::args().nth(1) {
        Some(path) => document = std::fs::read_to_string(path)?,
        None => {
            std::io::stdin().read_to_string(&mut document)?;
        }
    }
    let engine = numero_engine::Engine::default();
    let results = engine.evaluate(&document);
    let width = document.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    for (line, result) in document.split('\n').zip(results) {
        let display = result.display.unwrap_or_default();
        println!("{line:<width$}  │ {display}");
    }
    Ok(())
}
