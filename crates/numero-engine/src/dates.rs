//! Dates and clock times (§8): `today`, `now`, `2:30 pm HKT`, `Berlin
//! time`, `today + 3 weeks`, date differences. Evaluated against the
//! [`EvalContext`] clock so tests can pin it.
//!
//! As in Numi (§8 [oracle]), `today` is today at 12:00 local time,
//! `tomorrow` and `yesterday` are ±86,400 s from it, durations add as
//! seconds with the fixed ratios (a month is 30 days, a year 365), a plain
//! number adds seconds, and `date − date` is a duration in the largest
//! unit it fills. A clock time with a zone (`3pm EST`) is read on today's
//! local date and shown in the local zone; `in <zone>` only converts
//! `now`.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use chrono_tz::Tz;

use crate::EvalContext;
use crate::eval::{Scope, eval, quantity, unit_ref};
use crate::num::Num;
use crate::parser::{DateWord, Expr};
use crate::units::{Dim, UnitRef, UnitTable};
use crate::value::{Eval, Moment, Quantity, Unit, Value, fail};

const SECONDS_PER_DAY: i64 = 86_400;

fn localize(tz: Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    tz.from_local_datetime(&naive).earliest()
}

/// Today at 12:00 local time.
fn noon(ctx: &EvalContext) -> DateTime<Tz> {
    let now = ctx.now.with_timezone(&ctx.local_tz);
    now.date_naive()
        .and_hms_opt(12, 0, 0)
        .and_then(|naive| localize(ctx.local_tz, naive))
        .unwrap_or(now)
}

pub fn date_word(word: DateWord, ctx: &EvalContext) -> Value {
    let today = noon(ctx);
    let days = match word {
        DateWord::Today => 0,
        DateWord::Tomorrow => 1,
        DateWord::Yesterday => -1,
    };
    let at = today
        .checked_add_signed(Duration::seconds(days * SECONDS_PER_DAY))
        .unwrap_or(today);
    Value::Date(Moment { at })
}

/// How far back `oct 5` may lie before it means next year's: inferred from
/// Numi taking a date 39 days back as this year's and one 162 days back as
/// next year's (§8.1 [oracle]).
const PAST_DATE_DAYS: i64 = 90;

/// `5 oct`, `oct 5`: that day at 12:00 local time, this year unless that
/// lies more than [`PAST_DATE_DAYS`] back. Days past the month's end roll
/// over (`31 feb` is early March).
pub fn day_of_month(day: Value, month: u8, ctx: &EvalContext) -> Eval<Value> {
    let q = quantity(day)?;
    let Some(day) =
        q.n.to_i64()
            .filter(|d| (1..=31).contains(d) && q.unit == Unit::None)
    else {
        return fail("no such day");
    };
    let today = noon(ctx);
    let on = |year: i32| {
        chrono::NaiveDate::from_ymd_opt(year, u32::from(month), 1)
            .and_then(|first| first.checked_add_signed(Duration::days(day - 1)))
            .and_then(|date| date.and_hms_opt(12, 0, 0))
            .and_then(|naive| localize(ctx.local_tz, naive))
    };
    let at = on(today.year()).and_then(|this_year| {
        if (today - this_year).num_days() > PAST_DATE_DAYS {
            on(today.year() + 1)
        } else {
            Some(this_year)
        }
    });
    at.map_or_else(|| fail("no such day"), |at| Ok(Value::Date(Moment { at })))
}

pub fn now(zone: Option<Tz>, ctx: &EvalContext) -> Value {
    let tz = zone.unwrap_or(ctx.local_tz);
    Value::Date(Moment {
        at: ctx.now.with_timezone(&tz),
    })
}

/// Hour, minute and second of a clock-time expression (`14:30`, `2:30 pm`,
/// `3pm`), if it is one, and whether its half of the day is open
/// (`2:30`, but not `2:30 pm`, `14:30` or `09:30`).
fn clock_parts(expr: &Expr) -> Option<(u32, u32, u32, bool)> {
    match expr {
        Expr::Clock {
            hour,
            minute,
            second,
            padded,
        } => Some((*hour, *minute, *second, !padded && (1..12).contains(hour))),
        Expr::Meridiem(inner, pm) => {
            let (hour, minute, second) = match inner.as_ref() {
                Expr::Num(n, _) => (u32::try_from(n.to_i64()?).ok()?, 0, 0),
                other => {
                    let (hour, minute, second, _) = clock_parts(other)?;
                    (hour, minute, second)
                }
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
            Some((hour, minute, second, false))
        }
        _ => None,
    }
}

/// An hour written without am/pm reads as PM when its morning time has
/// passed and its afternoon time falls in the current hour or later:
/// at 9:41 PM, `9:00` and `11:59` are PM but `2:30` and `8:00` are AM
/// (§8.1 [oracle]).
fn resolve_half_day(hour: u32, minute: u32, second: u32, ctx: &EvalContext) -> u32 {
    let now = ctx.now.with_timezone(&ctx.local_tz).time();
    let morning = NaiveTime::from_hms_opt(hour, minute, second);
    if morning.is_some_and(|t| t < now) && hour + 12 >= now.hour() {
        hour + 12
    } else {
        hour
    }
}

/// A clock time on today's local date, read in `zone` (or the local
/// zone) and shown in the local zone.
pub fn clock(expr: &Expr, zone: Option<Tz>, ctx: &EvalContext) -> Eval<Value> {
    let Some((hour, minute, second, open)) = clock_parts(expr) else {
        return fail("not a time of day");
    };
    let hour = if open {
        resolve_half_day(hour, minute, second, ctx)
    } else {
        hour
    };
    let tz = zone.unwrap_or(ctx.local_tz);
    let date = ctx.now.with_timezone(&ctx.local_tz).date_naive();
    let at = date
        .and_hms_opt(hour, minute, second)
        .and_then(|naive| localize(tz, naive))
        .map(|at| at.with_timezone(&ctx.local_tz));
    at.map_or_else(|| fail("no such time"), |at| Ok(Value::Date(Moment { at })))
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
    })
}

/// `date ± duration`: the duration in whole seconds; a plain number
/// counts as seconds (§8.2 [oracle]).
pub fn shift(m: Moment, q: Quantity, subtract: bool, scope: &Scope<'_>) -> Eval<Value> {
    let n = if subtract { q.n.neg() } else { q.n };
    let seconds = match q.unit {
        Unit::None => n,
        Unit::Phys(unit) if scope.vocab.dim(unit) == Dim::Time => {
            let Some(second) = unit_ref(scope.vocab, "second") else {
                return fail("no seconds unit");
            };
            scope.vocab.convert(n, unit, second, scope.env).unwrap_or(n)
        }
        _ => return fail("add a duration to a date, such as 3 days"),
    };
    let Some(seconds) = seconds.round_dp(0).to_i64() else {
        return fail("duration out of range");
    };
    match m.at.checked_add_signed(Duration::seconds(seconds)) {
        Some(at) => Ok(Value::Date(Moment { at })),
        None => fail("date out of range"),
    }
}

/// Units a date difference is expressed in, largest first.
const DIFFERENCE_UNITS: [&str; 7] = ["year", "month", "week", "day", "hour", "minute", "second"];

/// `date − date`: a duration in the largest unit it fills at least once
/// (`1 day`, `8 h`, `1 w 3 day`), seconds when it is zero (§8.2 [oracle]).
pub fn difference(a: Moment, b: Moment, scope: &Scope<'_>) -> Value {
    let millis = (a.at - b.at).num_milliseconds();
    let seconds = Num::int(millis).div(Num::int(1000)).unwrap_or(Num::ZERO);
    let vocab = scope.vocab;
    let units: Vec<UnitRef> = DIFFERENCE_UNITS
        .iter()
        .filter_map(|id| unit_ref(vocab, id))
        .collect();
    let magnitude = seconds.abs().to_f64();
    let fits = |unit: &UnitRef| magnitude >= vocab.ratio(*unit, scope.env).to_f64();
    let Some(&unit) = units.iter().find(|u| fits(u)).or(units.last()) else {
        return Value::plain(seconds);
    };
    let n = seconds.div(vocab.ratio(unit, scope.env)).unwrap_or(seconds);
    Value::Q(Quantity::with(n, Unit::Phys(unit)))
}

/// `fromunix(seconds)` / `unixtime(seconds)`, shown in the local zone with
/// the UTC offset in effect at that instant. Numi applies today's offset
/// instead; Numero deliberately does not (spec, deliberate deviations).
pub fn from_unix(seconds: Value, ctx: &EvalContext) -> Eval<Value> {
    let q = quantity(seconds)?;
    let millis = q.n.mul(Num::int(1000)).round_dp(0).to_i64();
    match millis.and_then(DateTime::from_timestamp_millis) {
        Some(at) => Ok(Value::Date(Moment {
            at: at.with_timezone(&ctx.local_tz),
        })),
        None => fail("timestamp out of range"),
    }
}
