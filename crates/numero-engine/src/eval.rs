//! Evaluating an expression tree against the document's state (§2.4, §3).
//!
//! Units: `+ −` and `mod` convert the right operand into the left one's
//! unit (`5 m + 3 ft` = `5.91 m`); a bare number takes the other side's
//! unit. `×` keeps whichever unit there is, `÷` drops matching units into
//! a ratio and otherwise keeps the left unit. Beyond Numi, lengths
//! multiply into areas and volumes (`3 m × 4 m` = `12 m²`). A percentage
//! on the right of `+ − ×` is relative to the left (`100 + 20%` = 120).
//! Failures (incompatible units, missing rates, division by zero) leave
//! the line without a result, as Numi's NaN does.

use std::collections::HashMap;

use crate::dates;
use crate::ext::ExtFunction;
use crate::functions;
use crate::num::Num;
use crate::parser::{BinOp, Expr, Target};
use crate::units::{UnitEnv, UnitRef, UnitTable};
use crate::value::{Eval, Quantity, Unit, Value, fail};
use crate::vocab::{PctOp, Vocab};
use crate::{EvalContext, Rates};

/// π as Numi stores it, from the double.
pub const PI: &str = "3.141592653589793";
/// e to the 28 digits a decimal holds.
pub const E: &str = "2.718281828459045235360287471";

/// Everything an expression can refer to.
#[derive(Debug)]
pub struct Scope<'a> {
    pub rates: &'a Rates,
    pub vocab: &'a Vocab,
    pub ctx: &'a EvalContext,
    pub vars: &'a HashMap<String, Value>,
    /// The result of the line directly above, if it had one.
    pub prev: Option<&'a Value>,
    /// The results directly above, up to the nearest line without one.
    pub block: &'a [Value],
    pub env: UnitEnv,
    pub functions: &'a [ExtFunction],
}

pub fn eval(expr: &Expr, scope: &Scope<'_>) -> Eval<Value> {
    match expr {
        Expr::Num(n, fmt) => Ok(Value::Q(Quantity {
            n: *n,
            unit: Unit::None,
            fmt: *fmt,
        })),
        Expr::Var(name) => scope
            .vars
            .get(name)
            .copied()
            .map_or_else(|| fail(format!("unknown {name}")), Ok),
        Expr::Pi => Ok(Value::plain(Num::exact(PI))),
        Expr::E => Ok(Value::plain(Num::exact(E))),
        Expr::Prev => scope
            .prev
            .copied()
            .map_or_else(|| fail("no previous result"), Ok),
        Expr::Sum => total(scope),
        Expr::Avg => average(scope),
        Expr::Date(word) => Ok(dates::date_word(*word, scope.ctx)),
        Expr::Now(zone) => Ok(dates::now(*zone, scope.ctx)),
        Expr::Clock { .. } | Expr::Meridiem(..) => dates::clock(expr, None, scope.ctx),
        Expr::InZone(inner, tz) => dates::in_zone(inner, *tz, scope),
        Expr::Neg(inner) => negate(eval(inner, scope)?),
        Expr::Bin(op, a, b) => binary(*op, eval(a, scope)?, eval(b, scope)?, scope),
        Expr::Pct(op, a, b) => percent_op(
            *op,
            quantity(eval(a, scope)?)?,
            quantity(eval(b, scope)?)?,
            scope,
        ),
        Expr::Percent(inner) => with_unit(eval(inner, scope)?, Unit::Percent),
        Expr::Unit(inner, unit) => with_unit(eval(inner, scope)?, Unit::Phys(*unit)),
        Expr::Currency(inner, c) => with_currency(eval(inner, scope)?, *c, scope),
        Expr::Scale(inner, factor) => {
            let q = quantity(eval(inner, scope)?)?;
            Ok(Value::Q(q.map(q.n.mul(*factor))))
        }
        Expr::Call(func, args) => {
            let values = args
                .iter()
                .map(|a| eval(a, scope))
                .collect::<Eval<Vec<_>>>()?;
            functions::call(*func, &values, scope)
        }
        Expr::Convert(inner, target) => convert(eval(inner, scope)?, *target, scope),
    }
}

pub fn quantity(value: Value) -> Eval<Quantity> {
    match value {
        Value::Q(q) => Ok(q),
        Value::Date(_) => fail("expected a number, got a date"),
    }
}

fn negate(value: Value) -> Eval<Value> {
    let q = quantity(value)?;
    Ok(Value::Q(q.map(q.n.neg())))
}

fn with_unit(value: Value, unit: Unit) -> Eval<Value> {
    let q = quantity(value)?;
    match q.unit {
        Unit::None => Ok(Value::Q(Quantity { unit, ..q })),
        existing if existing == unit => Ok(Value::Q(q)),
        _ => fail("a value cannot take two units"),
    }
}

fn with_currency(value: Value, code: u16, scope: &Scope<'_>) -> Eval<Value> {
    let q = quantity(value)?;
    match q.unit {
        Unit::None => Ok(Value::Q(Quantity {
            unit: Unit::Currency(code),
            ..q
        })),
        Unit::Currency(from) => Ok(Value::Q(Quantity::with(
            currency(q.n, from, code, scope)?,
            Unit::Currency(code),
        ))),
        _ => fail("a value cannot take two units"),
    }
}

/// `sum`: the block folded with `+` from its top line down, so the result
/// takes the top line's unit (§5).
fn total(scope: &Scope<'_>) -> Eval<Value> {
    let mut values = scope.block.iter().copied();
    let Some(first) = values.next() else {
        return Ok(Value::plain(Num::ZERO));
    };
    values.try_fold(first, |acc, v| binary(BinOp::Add, acc, v, scope))
}

fn average(scope: &Scope<'_>) -> Eval<Value> {
    if scope.block.is_empty() {
        return Ok(Value::plain(Num::ZERO));
    }
    let count = Num::int(i64::try_from(scope.block.len()).unwrap_or(i64::MAX));
    let sum = quantity(total(scope)?)?;
    let n = sum
        .n
        .div(count)
        .map_or_else(|| fail("nothing to average"), Ok)?;
    Ok(Value::Q(sum.map(n)))
}

pub fn binary(op: BinOp, a: Value, b: Value, scope: &Scope<'_>) -> Eval<Value> {
    match (a, b) {
        (Value::Q(a), Value::Q(b)) => arith(op, a, b, scope).map(Value::Q),
        (Value::Date(d), Value::Q(q)) if matches!(op, BinOp::Add | BinOp::Sub) => {
            dates::shift(d, q, op == BinOp::Sub, scope)
        }
        (Value::Q(q), Value::Date(d)) if op == BinOp::Add => dates::shift(d, q, false, scope),
        (Value::Date(a), Value::Date(b)) if op == BinOp::Sub => {
            Ok(dates::difference(a, b, scope.vocab))
        }
        _ => fail("cannot combine a date that way"),
    }
}

fn arith(op: BinOp, a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Quantity> {
    // The result keeps the left operand's attributes, numeral system included.
    let fmt = a.fmt;
    let result = match op {
        BinOp::Add => add(a, b, false, scope),
        BinOp::Sub => add(a, b, true, scope),
        BinOp::Mul => multiply(a, b, scope),
        BinOp::Div => divide(a, b, scope),
        BinOp::Mod => modulo(a, b, scope),
        BinOp::Pow => power(a, b, scope),
        BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => bitwise(op, a, b),
    }?;
    Ok(Quantity { fmt, ..result })
}

fn add(a: Quantity, b: Quantity, subtract: bool, scope: &Scope<'_>) -> Eval<Quantity> {
    let combine = |x: Num, y: Num| if subtract { x.sub(y) } else { x.add(y) };
    match (a.unit, b.unit) {
        (Unit::Percent, Unit::Percent) | (Unit::None, Unit::None) | (Unit::Percent, Unit::None) => {
            Ok(a.map(combine(a.n, b.n)))
        }
        // PercentageAddition/Subtraction: `x ± x × p/100`.
        (_, Unit::Percent) => Ok(a.map(combine(a.n, a.n.mul(fraction(b.n))))),
        (Unit::None, _) => Ok(b.map(combine(a.n, b.n))),
        (_, Unit::None) => Ok(a.map(combine(a.n, b.n))),
        _ => {
            let b_n = convert_quantity(b, a.unit, scope)?;
            Ok(a.map(combine(a.n, b_n)))
        }
    }
}

fn fraction(percent: Num) -> Num {
    percent.div(Num::HUNDRED).unwrap_or(Num::ZERO)
}

fn multiply(a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Quantity> {
    let n = a.n.mul(b.n);
    let units = scope.vocab;
    match (a.unit, b.unit) {
        (Unit::Percent, Unit::Percent) => Ok(a.map(fraction(n))),
        // PercentageMultiplication: `x × p/100`, in x's unit.
        (Unit::Percent, _) => Ok(b.map(fraction(n))),
        (_, Unit::Percent) => Ok(a.map(fraction(n))),
        (Unit::None, _) => Ok(b.map(n)),
        (Unit::Phys(x), Unit::Phys(y))
            if units.is_length_family(x) && units.is_length_family(y) && x.power + y.power <= 3 =>
        {
            let b_n = units
                .convert(b.n, y, x.with_power(y.power), scope.env)
                .unwrap_or(b.n);
            Ok(Quantity::with(
                a.n.mul(b_n),
                Unit::Phys(x.with_power(x.power + y.power)),
            ))
        }
        _ => Ok(a.map(n)),
    }
}

fn divide(a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Quantity> {
    let quotient = |x: Num, y: Num| x.div(y).map_or_else(|| fail("division by zero"), Ok);
    let units = scope.vocab;
    match (a.unit, b.unit) {
        (_, Unit::None) => Ok(a.map(quotient(a.n, b.n)?)),
        (Unit::Percent, Unit::Percent) => Ok(Quantity::plain(quotient(a.n, b.n)?)),
        // No PercentageDivision: the percentage divides as a plain number.
        (_, Unit::Percent) => Ok(a.map(quotient(a.n, b.n)?)),
        (Unit::Phys(x), Unit::Phys(y))
            if units.dim(x) != units.dim(y)
                && units.is_length_family(x)
                && units.is_length_family(y)
                && x.power > y.power =>
        {
            let b_n = units
                .convert(b.n, y, x.with_power(y.power), scope.env)
                .unwrap_or(b.n);
            Ok(Quantity::with(
                quotient(a.n, b_n)?,
                Unit::Phys(x.with_power(x.power - y.power)),
            ))
        }
        _ => match convert_quantity(b, a.unit, scope) {
            Ok(b_n) => Ok(Quantity::plain(quotient(a.n, b_n)?)),
            Err(_) => Ok(a.map(quotient(a.n, b.n)?)),
        },
    }
}

fn modulo(a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Quantity> {
    let b_n = match b.unit {
        Unit::None | Unit::Percent => b.n,
        _ => convert_quantity(b, a.unit, scope)?,
    };
    a.n.rem(b_n)
        .map_or_else(|| fail("division by zero"), |n| Ok(a.map(n)))
}

fn power(a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Quantity> {
    let n = a.n.pow(b.n);
    if !n.is_finite() {
        return fail("result is not a number");
    }
    match (a.unit, b.n.to_i64()) {
        (Unit::Phys(u), Some(p @ 2..=3)) if scope.vocab.is_length_family(u) && u.power == 1 => Ok(
            Quantity::with(n, Unit::Phys(u.with_power(u8::try_from(p).unwrap_or(1)))),
        ),
        _ => Ok(a.map(n)),
    }
}

/// `& | xor << >>` on the integer parts, as Numi's `integerValue` (§2.4).
fn bitwise(op: BinOp, a: Quantity, b: Quantity) -> Eval<Quantity> {
    let integer = |n: Num| n.map_integer(|d| d.trunc(), f64::trunc).to_i64();
    let (Some(x), Some(y)) = (integer(a.n), integer(b.n)) else {
        return fail("number too large for a bitwise operator");
    };
    let shift = |y: i64| u32::try_from(y).ok().filter(|s| *s < 64);
    let result = match op {
        BinOp::And => x & y,
        BinOp::Or => x | y,
        BinOp::Xor => x ^ y,
        BinOp::Shl => shift(y).map_or(0, |s| x.wrapping_shl(s)),
        _ => shift(y).map_or(0, |s| x >> s),
    };
    Ok(a.map(Num::int(result)))
}

/// The twelve percent operations of §3, minus the three `x ± × p%` forms
/// that live in [`add`] and [`multiply`].
fn percent_op(op: PctOp, a: Quantity, b: Quantity, scope: &Scope<'_>) -> Eval<Value> {
    let divided = |x: Num, y: Num| x.div(y).map_or_else(|| fail("division by zero"), Ok);
    if op.needs_percent_left() {
        if a.unit != Unit::Percent {
            return fail("expected a percentage");
        }
        let p = a.n;
        let y = b.n;
        let n = match op {
            PctOp::Of => y.mul(fraction(p)),
            PctOp::On => y.add(y.mul(fraction(p))),
            PctOp::Off => y.sub(y.mul(fraction(p))),
            PctOp::OfWhatIs => divided(y, fraction(p))?,
            PctOp::OnWhatIs => divided(y.mul(Num::HUNDRED), Num::HUNDRED.add(p))?,
            _ => divided(y.mul(Num::HUNDRED), Num::HUNDRED.sub(p))?,
        };
        return Ok(Value::Q(b.map(n)));
    }
    // `a as a % of b`: `a` is converted to `b`'s unit first.
    let a_n = match (a.unit, b.unit) {
        (Unit::None, _) | (_, Unit::None) => a.n,
        _ => convert_quantity(a, b.unit, scope)?,
    };
    let b_n = b.n;
    let n = match op {
        PctOp::AsOf => divided(a_n, b_n)?.mul(Num::HUNDRED),
        PctOp::AsOn => divided(a_n.sub(b_n), b_n)?.mul(Num::HUNDRED),
        _ => divided(b_n.sub(a_n), b_n)?.mul(Num::HUNDRED),
    };
    Ok(Value::Q(Quantity::with(n, Unit::Percent)))
}

/// `q`'s number expressed in `unit`.
fn convert_quantity(q: Quantity, unit: Unit, scope: &Scope<'_>) -> Eval<Num> {
    let units = scope.vocab;
    match (q.unit, unit) {
        (Unit::Phys(from), Unit::Phys(to)) if units.dim(from) == units.dim(to) => units
            .convert(q.n, from, to, scope.env)
            .map_or_else(|| fail("cannot convert"), Ok),
        (Unit::Currency(from), Unit::Currency(to)) => currency(q.n, from, to, scope),
        (from, to) if from == to => Ok(q.n),
        _ => fail("incompatible units"),
    }
}

/// Units of a currency one US dollar buys.
fn rate(code: u16, scope: &Scope<'_>) -> Eval<Num> {
    let name = scope
        .vocab
        .currencies
        .get(usize::from(code))
        .map_or("", |c| c.code.as_str());
    if name == "USD" {
        return Ok(Num::ONE);
    }
    match scope
        .rates
        .per_usd
        .get(name)
        .filter(|r| r.is_finite() && **r > 0.0)
    {
        Some(r) => Ok(Num::parse(&r.to_string()).unwrap_or(Num::Float(*r))),
        None => fail(format!("no exchange rate for {name}")),
    }
}

pub fn currency(n: Num, from: u16, to: u16, scope: &Scope<'_>) -> Eval<Num> {
    if from == to {
        return Ok(n);
    }
    let usd = n
        .div(rate(from, scope)?)
        .map_or_else(|| fail("no exchange rate"), Ok)?;
    Ok(usd.mul(rate(to, scope)?))
}

fn convert(value: Value, target: Target, scope: &Scope<'_>) -> Eval<Value> {
    match (value, target) {
        (Value::Date(m), Target::Zone(tz)) => Ok(dates::to_zone(m, tz)),
        (Value::Date(m), Target::Local) => Ok(dates::to_zone(m, scope.ctx.local_tz)),
        (Value::Date(_), _) => fail("dates convert only to time zones"),
        (Value::Q(q), Target::Format(fmt)) => Ok(Value::Q(Quantity { fmt, ..q })),
        (Value::Q(q), Target::Unit(unit)) => convert_to(q, Unit::Phys(unit), scope),
        (Value::Q(q), Target::Currency(code)) => convert_to(q, Unit::Currency(code), scope),
        (Value::Q(q), Target::Percent) => convert_to(q, Unit::Percent, scope),
        (Value::Q(_), Target::Zone(_) | Target::Local) => fail("only times convert to a time zone"),
    }
}

/// `in <unit>`: a bare number takes the unit; a quantity converts within
/// its base unit (§6).
fn convert_to(q: Quantity, unit: Unit, scope: &Scope<'_>) -> Eval<Value> {
    let n = match q.unit {
        Unit::None => q.n,
        _ => convert_quantity(q, unit, scope)?,
    };
    Ok(Value::Q(Quantity { n, unit, ..q }))
}

/// `UnitRef` for a unit id of the built-in table, for tests and helpers.
pub fn unit_ref(vocab: &Vocab, id: &str) -> Option<UnitRef> {
    crate::units::unit_index(&vocab.units, id).map(UnitRef::plain)
}
