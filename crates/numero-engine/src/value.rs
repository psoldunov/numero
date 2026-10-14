//! Calculated values and how they read in the result column.

use chrono::{DateTime, Timelike};
use chrono_tz::Tz;

use crate::format;
use crate::num::Num;
use crate::units::{Dim, UnitRef, UnitTable, unit_index};
use crate::vocab::{NumFmt, Vocab};
use crate::{Config, EvalContext};

/// The chain a fractional duration is spelled out in (§1.2, §7 [oracle]).
const DURATION_CHAIN: [&str; 7] = ["year", "month", "week", "day", "hour", "minute", "second"];

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

/// A point in time, in the zone it is shown in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    pub at: DateTime<Tz>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Q(Quantity),
    Date(Moment),
}

/// Why a line has no result. `empty` is Numi's empty value (a function
/// called with the wrong arguments), which starts the grand total over;
/// any other failure is NaN, which empties it for good (§5 [oracle]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct EvalError {
    pub message: String,
    pub empty: bool,
}

pub type Eval<T> = Result<T, EvalError>;

pub fn fail<T>(message: impl Into<String>) -> Eval<T> {
    Err(EvalError {
        message: message.into(),
        empty: false,
    })
}

pub fn fail_empty<T>(message: impl Into<String>) -> Eval<T> {
    Err(EvalError {
        message: message.into(),
        empty: true,
    })
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
    if let Some(compound) = compound_duration(q, vocab, config) {
        return compound;
    }
    let currency = matches!(q.unit, Unit::Currency(_));
    let number = format::with_format(q.n.abs(), q.fmt, config, currency);
    let body = match q.unit {
        Unit::None => number.clone(),
        Unit::Percent => apply_template("%%", &number),
        Unit::Phys(unit) => apply_template(&vocab.unit_format(unit), &number),
        Unit::Currency(c) => apply_template(&vocab.currency_format(c), &number),
    };
    // A negative value keeps its sign even when it rounds to zero:
    // `-0.001` shows `-0` (§7 [oracle]).
    if q.n.is_negative() {
        format!("-{body}")
    } else {
        body
    }
}

/// A time value with a fractional amount, spelled out from its own unit
/// down to whole seconds: `1.5 h` → `1 h 30 min`, `400 days in years` →
/// `1 yr 1 mon. 5 day`. Zero parts are skipped except the first; every
/// non-zero part carries the sign (`-1 h -30 min`, `0 h -30 min`). Whole
/// amounts and seconds print plainly (`1.5 s`).
fn compound_duration(q: &Quantity, vocab: &Vocab, config: &Config) -> Option<String> {
    let Unit::Phys(unit) = q.unit else {
        return None;
    };
    if q.fmt != NumFmt::Decimal || q.n.is_integer() || unit.prefix.is_some() || unit.power != 1 {
        return None;
    }
    if vocab.dim(unit) != Dim::Time {
        return None;
    }
    let start = DURATION_CHAIN
        .iter()
        .position(|id| vocab.unit_def(unit).id == *id)
        .filter(|&i| i + 1 < DURATION_CHAIN.len())?;
    let ratio = |id: &str| {
        unit_index(&vocab.units, id)
            .map(|i| vocab.ratio(UnitRef::plain(i), crate::doc::DEFAULT_ENV))
    };
    let sign = if q.n.is_negative() { "-" } else { "" };
    // Seconds to a millionth first, so `3661 s in h` (1.01694… h) spells
    // `1 h 1 min 1 s`; a real fraction of a second is dropped.
    let mut remaining = q.n.abs().mul(ratio(DURATION_CHAIN[start])?).round_dp(6);
    let mut parts = Vec::new();
    for (i, id) in DURATION_CHAIN.iter().enumerate().skip(start) {
        let size = ratio(id)?;
        let count = remaining.div(size)?.map_integer(|d| d.trunc(), f64::trunc);
        remaining = remaining.sub(count.mul(size));
        if i == start || !count.is_zero() {
            let unit = UnitRef::plain(unit_index(&vocab.units, id)?);
            let number = format::number(count, config, false);
            let sign = if count.is_zero() { "" } else { sign };
            parts.push(format!(
                "{sign}{}",
                apply_template(&vocab.unit_format(unit), &number)
            ));
        }
    }
    Some(parts.join(" "))
}

/// en_US short styles (§8.4 [oracle]): a value at 12:00 shows only its
/// date (that is how `today` reads), a value on today's local date shows
/// only the time, and anything else shows both.
fn display_moment(m: &Moment, ctx: &EvalContext) -> String {
    if m.at.hour() == 12 && m.at.minute() == 0 {
        return m.at.format("%-m/%-d/%y").to_string();
    }
    let today = ctx.now.with_timezone(&ctx.local_tz).date_naive();
    if m.at.date_naive() == today {
        m.at.format("%-I:%M %p").to_string()
    } else {
        m.at.format("%-m/%-d/%y, %-I:%M %p").to_string()
    }
}
