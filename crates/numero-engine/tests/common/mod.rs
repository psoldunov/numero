//! Shared fixtures: a pinned clock and a table checker.
//!
//! The clock is 2026-10-14 12:00 UTC in Europe/Berlin (CEST, 2:00 PM).

#![allow(dead_code)]

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use numero_engine::{Config, Engine, EvalContext, LineKind, LineResult, Rates};

pub fn ctx() -> EvalContext {
    EvalContext {
        now: Utc
            .with_ymd_and_hms(2026, 10, 14, 12, 0, 0)
            .single()
            .expect("valid instant"),
        local_tz: chrono_tz::Europe::Berlin,
    }
}

pub fn rates() -> Rates {
    let per_usd: HashMap<String, f64> = [
        ("EUR", 0.9),
        ("GBP", 0.8),
        ("JPY", 150.0),
        ("BTC", 0.00001),
        ("ETH", 0.0004),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    Rates {
        per_usd,
        fetched_at: None,
    }
}

pub fn engine() -> Engine {
    let mut engine = Engine::new(Config::default());
    engine.set_rates(rates());
    engine
}

/// The result column as a test reads it: the display text, `ERR` for an
/// error line, `` for a line with no result.
pub fn shown(result: &LineResult) -> String {
    match result.kind {
        LineKind::Error => "ERR".into(),
        _ => result.display.clone().unwrap_or_default(),
    }
}

/// Check the last line of each document against its expected result.
pub fn check_with(engine: &Engine, cases: &[(&str, &str)]) {
    let ctx = ctx();
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(doc, expected)| {
            let results = engine.evaluate_with(doc, &ctx);
            let got = results.last().map(shown).unwrap_or_default();
            (got != *expected).then(|| format!("{doc:?}: expected {expected:?}, got {got:?}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

pub fn check(cases: &[(&str, &str)]) {
    check_with(&engine(), cases);
}
