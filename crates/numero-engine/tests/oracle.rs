//! Parity with Numi 3.35's real engine.
//!
//! `fixtures/numi-oracle.tsv` holds what Numi displays for every line of the
//! oracle corpora (`~/re/numi/oracle`, regenerated with its
//! `py/fixture.py`). Each document is evaluated here against the clock its
//! source was run at, in Europe/Berlin, at precision 2, and every result
//! line and grand total must match Numi's exactly. Documents marked `rates`
//! come from Numi's online run: they get fixed exchange rates and compare
//! by shape, every number standing in for any other. All other documents
//! come from the offline run and get no rates, as Numi had none.
//!
//! Differences that are deliberate, or not fixed yet, are listed in
//! [`ALLOWED`] with a reason. A new mismatch fails the test, and so does an
//! allowed one that has started to match.

mod common;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use numero_engine::{Config, Engine, EvalContext, LineKind, Rates};

const FIXTURE: &str = include_str!("fixtures/numi-oracle.tsv");

/// `source:doc:line` → why Numero's result differs from Numi's.
const ALLOWED: &[(&str, &str)] = &[
    // Deliberate: `fromunix` uses the UTC offset in effect at that date;
    // Numi applies today's offset (docs/numi-spec.md, deliberate deviations).
    ("corpus:554:0", "fromunix: offset at that date, not today's"),
    ("corpus:555:0", "fromunix: offset at that date, not today's"),
    ("corpus:556:0", "unixtime: offset at that date, not today's"),
    ("probes:153:0", "fromunix: offset at that date, not today's"),
    ("probes:154:0", "fromunix: offset at that date, not today's"),
    (
        "probes:172:0",
        "today - fromunix(0): offset at that date, not today's",
    ),
    (
        "probes2:192:0",
        "fromunix: offset at that date, not today's",
    ),
    (
        "probes2:193:0",
        "unixtime: offset at that date, not today's",
    ),
    // Deliberate: Numi looks city names up at s1.numi.app, which gave the
    // oracle no answer; Numero ships an offline city table (spec §8.3).
    (
        "probes:174:0",
        "city time zone: offline table, Numi's lookup gave nothing",
    ),
    (
        "probes:175:0",
        "city time zone: offline table, Numi's lookup gave nothing",
    ),
    (
        "probes:176:0",
        "city time zone: offline table, Numi's lookup gave nothing",
    ),
];

struct Row {
    line: String,
    category: String,
    rates: bool,
    input: String,
    numi: String,
}

struct Document {
    source: String,
    doc: String,
    rows: Vec<Row>,
}

fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Documents in fixture order, and the run instant of every source.
fn load() -> (Vec<Document>, HashMap<String, DateTime<Utc>>) {
    let mut clocks = HashMap::new();
    let mut docs: Vec<Document> = Vec::new();
    for raw in FIXTURE.lines() {
        if let Some(clock) = raw.strip_prefix("#clock ") {
            let (source, instant) = clock.split_once(' ').expect("#clock source instant");
            let instant = DateTime::parse_from_rfc3339(instant).expect("RFC 3339 instant");
            clocks.insert(source.to_string(), instant.with_timezone(&Utc));
            continue;
        }
        if raw.starts_with('#') || raw.starts_with("source\t") {
            continue;
        }
        let fields: Vec<&str> = raw.split('\t').collect();
        let [source, doc, line, category, mode, input, numi] = fields[..] else {
            panic!("malformed fixture row: {raw:?}");
        };
        let row = Row {
            line: line.to_string(),
            category: category.to_string(),
            rates: mode == "rates",
            input: unescape(input),
            numi: unescape(numi),
        };
        match docs.last_mut() {
            Some(last) if last.source == source && last.doc == doc => last.rows.push(row),
            _ => docs.push(Document {
                source: source.to_string(),
                doc: doc.to_string(),
                rows: vec![row],
            }),
        }
    }
    (docs, clocks)
}

/// Every run of digits (with grouping and a fraction) as `9`.
fn shape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            out.push(c);
            continue;
        }
        out.push('9');
        while let Some(&next) = chars.peek() {
            let continues = next.is_ascii_digit()
                || ((next == ',' || next == '.') && {
                    let mut ahead = chars.clone();
                    ahead.next();
                    ahead.peek().is_some_and(char::is_ascii_digit)
                });
            if !continues {
                break;
            }
            chars.next();
        }
    }
    out
}

/// Numi puts U+202F before AM/PM; Numero uses a plain space on purpose.
fn normalise(text: &str) -> String {
    text.replace('\u{202f}', " ")
}

#[test]
fn numero_matches_numi_on_the_oracle_corpora() {
    let (docs, clocks) = load();
    let allowed: HashMap<&str, &str> = ALLOWED.iter().copied().collect();
    let plain = Engine::new(Config::default());
    let mut with_rates = Engine::new(Config::default());
    with_rates.set_rates(common::rates());
    let no_rates = Rates::default();
    assert_eq!(plain.rates(), &no_rates);

    let mut unexpected = Vec::new();
    let mut matched_allowed = Vec::new();
    let mut compared = 0;
    for doc in &docs {
        let now = clocks[&doc.source];
        let ctx = EvalContext {
            now,
            local_tz: chrono_tz::Europe::Berlin,
        };
        let rates = doc.rows.iter().any(|r| r.rates);
        let engine = if rates { &with_rates } else { &plain };
        let text: Vec<&str> = doc
            .rows
            .iter()
            .filter(|r| r.line != "total")
            .map(|r| r.input.as_str())
            .collect();
        let evaluation = engine.evaluate_document(&text.join("\n"), &ctx);
        for row in &doc.rows {
            compared += 1;
            let got = if row.line == "total" {
                evaluation.grand_total.clone().unwrap_or_default()
            } else {
                let index: usize = row.line.parse().expect("line index");
                let result = &evaluation.lines[index];
                match result.kind {
                    LineKind::Error => String::new(),
                    _ => result.display.clone().unwrap_or_default(),
                }
            };
            let numi = normalise(&row.numi);
            let same = if rates {
                shape(&numi) == shape(&got)
            } else {
                numi == got
            };
            let key = format!("{}:{}:{}", doc.source, doc.doc, row.line);
            let shown = if row.line == "total" {
                format!("Total of {:?}", text.join(" ⏎ "))
            } else {
                format!("{:?}", row.input)
            };
            match (same, allowed.contains_key(key.as_str())) {
                (true, true) => matched_allowed.push(key),
                (false, false) => unexpected.push(format!(
                    "{key} [{}] {shown}: Numi {numi:?}, Numero {got:?}",
                    row.category
                )),
                _ => {}
            }
        }
    }
    let known: Vec<String> = docs
        .iter()
        .flat_map(|d| {
            d.rows
                .iter()
                .map(move |r| format!("{}:{}:{}", d.source, d.doc, r.line))
        })
        .collect();
    let stale: Vec<&str> = ALLOWED
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !known.iter().any(|k| k == key))
        .collect();
    assert!(
        unexpected.is_empty() && matched_allowed.is_empty() && stale.is_empty(),
        "{compared} results compared.\n\
         {} differ from Numi and are not allowed:\n{}\n\
         {} allowed differences now match (remove them from ALLOWED):\n{}\n\
         {} ALLOWED keys name no fixture row:\n{}",
        unexpected.len(),
        unexpected.join("\n"),
        matched_allowed.len(),
        matched_allowed.join("\n"),
        stale.len(),
        stale.join("\n"),
    );
}

#[test]
fn shapes_hide_numbers_only() {
    assert_eq!(shape("€ 7.48"), "€ 9");
    assert_eq!(shape("$81,657.99"), "$9");
    assert_eq!(shape("1, 2"), "9, 9");
    assert_eq!(shape("9:22 PM"), "9:9 PM");
}
