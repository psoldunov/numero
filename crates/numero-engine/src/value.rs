//! Calculated values and how they read in the result column.

use chrono::DateTime;
use chrono_tz::Tz;

use crate::format;
use crate::num::Num;
use crate::units::UnitRef;
use crate::vocab::{NumFmt, Vocab};
use crate::{Config, EvalContext};

/// What a number measures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    None,
    Percent,
    Phys(UnitRef),
    Currency(u16),
}

/// A number with a unit and the representation the line asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub n: Num,
    pub unit: Unit,
    pub fmt: NumFmt,
}

/// A point in time; `has_time` is false for whole days (`today`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    pub at: DateTime<Tz>,
    pub has_time: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Q(Quantity),
    Date(Moment),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EvalError(pub String);

pub type Eval<T> = Result<T, EvalError>;

pub fn fail<T>(message: impl Into<String>) -> Eval<T> {
    Err(EvalError(message.into()))
}

impl Quantity {
    pub fn plain(n: Num) -> Self {
        Self {
            n,
            unit: Unit::None,
            fmt: NumFmt::Decimal,
        }
    }

    pub fn with(n: Num, unit: Unit) -> Self {
        Self {
            n,
            unit,
            fmt: NumFmt::Decimal,
        }
    }

    pub fn map(self, n: Num) -> Self {
        Self { n, ..self }
    }
}

impl Value {
    pub fn plain(n: Num) -> Self {
        Value::Q(Quantity::plain(n))
    }

    pub fn number(&self) -> Option<f64> {
        match self {
            Value::Q(q) => Some(q.n.to_f64()),
            Value::Date(_) => None,
        }
    }

    pub fn display(&self, vocab: &Vocab, config: &Config, ctx: &EvalContext) -> String {
        match self {
            Value::Q(q) => display_quantity(q, vocab, config),
            Value::Date(m) => display_moment(m, ctx),
        }
    }
}

/// `<id>.format` rendering (§1.1, §7): a template with `%@` takes the
/// number in its place (`$%@` → `$5`); any other format follows the number
/// after a space (`5 m`, `36 ″`). `%%` is a literal percent sign, so the
/// percent format `%%` renders `20 %`.
fn apply_template(template: &str, number: &str) -> String {
    let filled = if template.contains("%@") {
        template.replace("%@", number)
    } else {
        format!("{number} {template}")
    };
    filled.replace("%%", "%")
}

fn display_quantity(q: &Quantity, vocab: &Vocab, config: &Config) -> String {
    let currency = matches!(q.unit, Unit::Currency(_));
    let number = format::with_format(q.n.abs(), q.fmt, config, currency);
    let body = match q.unit {
        Unit::None => number.clone(),
        Unit::Percent => apply_template("%%", &number),
        Unit::Phys(unit) => apply_template(&vocab.unit_format(unit), &number),
        Unit::Currency(c) => apply_template(&vocab.currency_format(c), &number),
    };
    let digits = ["0x", "0b", "0o"]
        .iter()
        .fold(number.as_str(), |n, prefix| n.trim_start_matches(prefix));
    let shows_zero = !digits
        .chars()
        .take_while(|c| *c != 'E')
        .any(|c| c.is_ascii_alphanumeric() && c != '0');
    if q.n.is_negative() && !shows_zero {
        format!("-{body}")
    } else {
        body
    }
}

/// en_US short styles (§8.4): a time today shows only the time, another
/// day shows date and time, and a date (`today`) shows the date.
fn display_moment(m: &Moment, ctx: &EvalContext) -> String {
    if !m.has_time {
        return m.at.format("%-m/%-d/%y").to_string();
    }
    let today = ctx.now.with_timezone(&m.at.timezone()).date_naive();
    if m.at.date_naive() == today {
        m.at.format("%-I:%M %p").to_string()
    } else {
        m.at.format("%-m/%-d/%y, %-I:%M %p").to_string()
    }
}
