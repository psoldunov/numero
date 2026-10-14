//! Dates and clock times (§8): `today`, `now`, `2:30 pm HKT`, `Berlin
//! time`, `today + 3 weeks`, date differences. Evaluated against the
//! [`EvalContext`] clock so tests can pin it.
//!
//! As in Numi, `today` is the current instant shown as a date, `tomorrow`
//! and `yesterday` are ±86,400 s, durations add as seconds with the fixed
//! ratios (a month is 30 days, a year 365), and `date − date` is seconds.

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

use crate::EvalContext;
use crate::eval::{Scope, eval, quantity, unit_ref};
use crate::num::Num;
use crate::parser::{DateWord, Expr};
use crate::units::{Dim, UnitTable};
use crate::value::{Eval, Moment, Quantity, Unit, Value, fail};
use crate::vocab::Vocab;

const SECONDS_PER_DAY: i64 = 86_400;

fn localize(tz: Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    tz.from_local_datetime(&naive).earliest()
}

pub fn date_word(word: DateWord, ctx: &EvalContext) -> Value {
    let now = ctx.now.with_timezone(&ctx.local_tz);
    let days = match word {
        DateWord::Today => 0,
        DateWord::Tomorrow => 1,
        DateWord::Yesterday => -1,
    };
    let at = now
        .checked_add_signed(Duration::seconds(days * SECONDS_PER_DAY))
        .unwrap_or(now);
    Value::Date(Moment {
        at,
        has_time: false,
    })
}

pub fn now(zone: Option<Tz>, ctx: &EvalContext) -> Value {
    let tz = zone.unwrap_or(ctx.local_tz);
    Value::Date(Moment {
        at: ctx.now.with_timezone(&tz),
        has_time: true,
    })
}

/// Hour, minute and second of a clock-time expression (`14:30`, `2:30 pm`,
/// `3pm`), if it is one.
fn clock_parts(expr: &Expr) -> Option<(u32, u32, u32)> {
    match expr {
        Expr::Clock {
            hour,
            minute,
            second,
        } => Some((*hour, *minute, *second)),
        Expr::Meridiem(inner, pm) => {
            let (hour, minute, second) = match inner.as_ref() {
                Expr::Num(n, _) => (u32::try_from(n.to_i64()?).ok()?, 0, 0),
                other => clock_parts(other)?,
            };
            if !(1..=12).contains(&hour) {
                return None;
            }
            let hour = match (*pm, hour) {
                (false, 12) => 0,
                (true, 12) => 12,
                (true, h) => h + 12,
                (false, h) => h,
            };
            Some((hour, minute, second))
        }
        _ => None,
    }
}

/// A clock time today, in `zone` or the local zone.
pub fn clock(expr: &Expr, zone: Option<Tz>, ctx: &EvalContext) -> Eval<Value> {
    let Some((hour, minute, second)) = clock_parts(expr) else {
        return fail("not a time of day");
    };
    let tz = zone.unwrap_or(ctx.local_tz);
    let date = ctx.now.with_timezone(&tz).date_naive();
    let at = date
        .and_hms_opt(hour, minute, second)
        .and_then(|naive| localize(tz, naive));
    at.map_or_else(
        || fail("no such time"),
        |at| Ok(Value::Date(Moment { at, has_time: true })),
    )
}

/// `2:30 pm HKT` reads the time in that zone; `now PST` converts.
pub fn in_zone(inner: &Expr, tz: Tz, scope: &Scope<'_>) -> Eval<Value> {
    if clock_parts(inner).is_some() {
        return clock(inner, Some(tz), scope.ctx);
    }
    match eval(inner, scope)? {
        Value::Date(m) => Ok(to_zone(m, tz)),
        Value::Q(_) => fail("only times have a time zone"),
    }
}

pub fn to_zone(m: Moment, tz: Tz) -> Value {
    Value::Date(Moment {
        at: m.at.with_timezone(&tz),
        has_time: true,
    })
}

/// `date ± duration`: the duration in whole seconds (§8.2).
pub fn shift(m: Moment, q: Quantity, subtract: bool, scope: &Scope<'_>) -> Eval<Value> {
    let Unit::Phys(unit) = q.unit else {
        return fail("add a duration to a date, such as 3 days");
    };
    if scope.vocab.dim(unit) != Dim::Time {
        return fail("add a duration to a date, such as 3 days");
    }
    let Some(second) = unit_ref(scope.vocab, "second") else {
        return fail("no seconds unit");
    };
    let n = if subtract { q.n.neg() } else { q.n };
    let seconds = scope.vocab.convert(n, unit, second, scope.env).unwrap_or(n);
    let Some(seconds) = seconds.round_dp(0).to_i64() else {
        return fail("duration out of range");
    };
    let has_time = m.has_time || seconds % SECONDS_PER_DAY != 0;
    match m.at.checked_add_signed(Duration::seconds(seconds)) {
        Some(at) => Ok(Value::Date(Moment { at, has_time })),
        None => fail("date out of range"),
    }
}

/// `date − date`, in seconds.
pub fn difference(a: Moment, b: Moment, vocab: &Vocab) -> Value {
    let millis = (a.at - b.at).num_milliseconds();
    let seconds = Num::int(millis).div(Num::int(1000)).unwrap_or(Num::ZERO);
    match unit_ref(vocab, "second") {
        Some(second) => Value::Q(Quantity::with(seconds, Unit::Phys(second))),
        None => Value::plain(seconds),
    }
}

/// `fromunix(seconds)` / `unixtime(seconds)`, shown in the local zone.
pub fn from_unix(seconds: Value, ctx: &EvalContext) -> Eval<Value> {
    let q = quantity(seconds)?;
    let millis = q.n.mul(Num::int(1000)).round_dp(0).to_i64();
    match millis.and_then(DateTime::from_timestamp_millis) {
        Some(at) => Ok(Value::Date(Moment {
            at: at.with_timezone(&ctx.local_tz),
            has_time: true,
        })),
        None => fail("timestamp out of range"),
    }
}
