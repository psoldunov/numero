//! Numero's calculation engine: a Numi-compatible, natural-language text
//! calculator. Pure Rust, no UI and no network; the app and the CLI feed it a
//! document and render one [`LineResult`] per line.
//!
//! The public surface below is the contract the GPUI app is built against.
//! Extend it freely; do not break it without updating `numero-app` and
//! `numero-cli` in the same change.

use std::collections::HashMap;
use std::ops::Range;

use chrono::{DateTime, Utc};

/// User-facing formatting and unit settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Maximum fraction digits shown in a result.
    pub precision: usize,
    /// Separator between the integer and fraction parts of a result.
    pub decimal_separator: char,
    /// Separator between groups of three integer digits, if any.
    pub grouping_separator: Option<char>,
    /// CSS pixels per inch, for `px`/`pt` conversions (`ppi` variable).
    pub ppi: f64,
    /// CSS pixels per `em` (`em` variable).
    pub em_px: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            precision: 2,
            decimal_separator: '.',
            grouping_separator: Some(','),
            ppi: 96.0,
            em_px: 16.0,
        }
    }
}

/// Exchange rates: how many units of each currency (ISO code, or crypto
/// ticker) one US dollar buys.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rates {
    pub per_usd: HashMap<String, f64>,
    /// When the rates were fetched, if known.
    pub fetched_at: Option<DateTime<Utc>>,
}

/// The clock and zone a document is evaluated against. Tests pin both.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalContext {
    pub now: DateTime<Utc>,
    pub local_tz: chrono_tz::Tz,
}

/// What a line of the document turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Blank line. Resets `sum`/`avg` blocks.
    Empty,
    /// `// comment` or a line with nothing calculable in it.
    Comment,
    /// `# Header`. Also resets `sum`/`avg` blocks.
    Header,
    /// A calculated value.
    Value,
    /// Text that looked calculable but failed (unknown unit, bad conversion).
    Error,
}

/// Syntax classes for highlighting a line in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenClass {
    Number,
    Unit,
    Currency,
    Operator,
    Keyword,
    Function,
    Variable,
    Label,
    Comment,
    Header,
    Text,
}

/// A highlighted byte range within one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub class: TokenClass,
}

/// The outcome of one line of the document.
#[derive(Debug, Clone, PartialEq)]
pub struct LineResult {
    pub kind: LineKind,
    /// Text for the result column, e.g. `12.7 cm`, `$ 9.00`, `14:05 CET`.
    pub display: Option<String>,
    /// The bare number behind `display`, for copying, if the result has one.
    pub number: Option<f64>,
    /// Highlight spans over the line's own bytes, in order, non-overlapping.
    pub spans: Vec<Span>,
}

impl LineResult {
    fn empty() -> Self {
        Self {
            kind: LineKind::Empty,
            display: None,
            number: None,
            spans: Vec::new(),
        }
    }
}

/// The engine: unit tables, rates and settings. Cheap to share; evaluation
/// does not mutate it.
#[derive(Debug, Clone, Default)]
pub struct Engine {
    config: Config,
    rates: Rates,
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            rates: Rates::default(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn set_rates(&mut self, rates: Rates) {
        self.rates = rates;
    }

    pub fn rates(&self) -> &Rates {
        &self.rates
    }

    /// Evaluate a whole document against the system clock and local zone.
    /// Returns exactly one result per `\n`-separated line.
    pub fn evaluate(&self, document: &str) -> Vec<LineResult> {
        let ctx = EvalContext {
            now: Utc::now(),
            local_tz: local_timezone(),
        };
        self.evaluate_with(document, &ctx)
    }

    /// Evaluate a whole document against an explicit clock and zone.
    pub fn evaluate_with(&self, document: &str, _ctx: &EvalContext) -> Vec<LineResult> {
        // Placeholder until the real evaluator lands: plain numbers only.
        document
            .split('\n')
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    LineResult::empty()
                } else if let Some(rest) = trimmed.strip_prefix('#') {
                    let _ = rest;
                    LineResult {
                        kind: LineKind::Header,
                        spans: vec![Span {
                            range: 0..line.len(),
                            class: TokenClass::Header,
                        }],
                        ..LineResult::empty()
                    }
                } else if let Ok(n) = trimmed.parse::<f64>() {
                    LineResult {
                        kind: LineKind::Value,
                        display: Some(n.to_string()),
                        number: Some(n),
                        spans: vec![Span {
                            range: 0..line.len(),
                            class: TokenClass::Number,
                        }],
                    }
                } else {
                    LineResult {
                        kind: LineKind::Comment,
                        ..LineResult::empty()
                    }
                }
            })
            .collect()
    }
}

/// The system's IANA zone, or UTC when it cannot be read.
pub fn local_timezone() -> chrono_tz::Tz {
    std::env::var("TZ")
        .ok()
        .and_then(|name| name.parse().ok())
        .or_else(|| {
            std::fs::read_link("/etc/localtime").ok().and_then(|path| {
                let path = path.to_string_lossy().into_owned();
                path.split("zoneinfo/").nth(1).and_then(|name| name.parse().ok())
            })
        })
        .unwrap_or(chrono_tz::UTC)
}
