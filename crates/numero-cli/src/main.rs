//! `numero [--fetch] [FILE]`: evaluate a document (a file, or stdin) and
//! print each line beside its result, then the grand total. Uses the cached exchange rates;
//! `--fetch` refreshes them from the network first.

use std::io::Read;
use std::process::ExitCode;

use numero_engine::{Config, Engine, EvalContext, LineKind};

const USAGE: &str = "usage: numero [--fetch] [FILE]\n\nEvaluates FILE (or stdin) line by line.\n  --fetch   refresh currency rates before evaluating";

struct Args {
    fetch: bool,
    path: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        fetch: false,
        path: None,
    };
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--fetch" => args.fetch = true,
            "-h" | "--help" => return Err(USAGE.into()),
            flag if flag.starts_with('-') && flag != "-" => {
                return Err(format!("unknown option {flag}\n\n{USAGE}"));
            }
            path if args.path.is_none() => args.path = Some(path.into()),
            _ => return Err(USAGE.into()),
        }
    }
    Ok(args)
}

fn read_document(path: Option<&str>) -> std::io::Result<String> {
    match path {
        Some(path) if path != "-" => std::fs::read_to_string(path),
        _ => {
            let mut document = String::new();
            std::io::stdin().read_to_string(&mut document)?;
            Ok(document)
        }
    }
}

fn load_rates(fetch: bool) -> Option<numero_engine::Rates> {
    if fetch {
        match numero_rates::fetch() {
            Ok(rates) => {
                if let Err(error) = numero_rates::save_cache(&rates) {
                    eprintln!("numero: could not cache rates: {error}");
                }
                return Some(rates);
            }
            Err(error) => eprintln!("numero: could not fetch rates: {error}"),
        }
    }
    numero_rates::load_cached()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let document = match read_document(args.path.as_deref()) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("numero: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut engine = Engine::new(Config::default());
    if let Some(rates) = load_rates(args.fetch) {
        engine.set_rates(rates);
    }
    let document = document.strip_suffix('\n').unwrap_or(&document);
    let evaluation = engine.evaluate_document(document, &EvalContext::system());
    let results = evaluation.lines;
    let width = document
        .split('\n')
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    for (line, result) in document.split('\n').zip(results) {
        let shown = match result.kind {
            LineKind::Error => "⚠".to_string(),
            _ => result.display.unwrap_or_default(),
        };
        println!("{line:<width$}  │ {shown}");
    }
    if let Some(total) = evaluation.grand_total {
        println!("{:<width$}  │ {total}", "Total");
    }
    ExitCode::SUCCESS
}
