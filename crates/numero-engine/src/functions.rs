//! Built-in functions (§9). `sin cos tan` read a unitless argument as
//! degrees and honour `rad`/`°`; `arcsin arccos arctan` answer in degrees.
//! `log x` is base 10 and `log b (x)` takes the base first; `root n (x)`
//! keeps the sign for odd roots of negatives; `fact x` is Γ(x + 1).
//! Extension functions run through [`crate::ext`].

use rust_decimal::{Decimal, RoundingStrategy};

use crate::dates;
use crate::eval::{Scope, quantity, unit_ref};
use crate::ext;
use crate::num::Num;
use crate::units::{Dim, UnitTable};
use crate::value::{Eval, Quantity, Unit, Value, fail};
use crate::vocab::Func;

/// Floats that land on a modest whole number become exact decimals again,
/// so `sqrt(16) + 0.1` stays decimal.
const EXACT_INTEGER_LIMIT: f64 = 1e15;
/// Largest `x` for `fact x`; beyond it Γ(x + 1) overflows a double.
const MAX_FACTORIAL: f64 = 170.0;

fn clean(f: f64) -> Eval<Num> {
    if !f.is_finite() {
        return fail("result is not a number");
    }
    if f.fract() == 0.0 && f.abs() < EXACT_INTEGER_LIMIT {
        return Ok(Num::int(f as i64));
    }
    Ok(Num::Float(f))
}

fn plain_arg(value: Value) -> Eval<f64> {
    Ok(quantity(value)?.n.to_f64())
}

fn arity(args: &[Value], min: usize, max: usize) -> Eval<()> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        fail("wrong number of arguments")
    }
}

pub fn call(func: Func, args: &[Value], scope: &Scope<'_>) -> Eval<Value> {
    if let Func::Custom(index) = func {
        return ext::call(index, args, scope);
    }
    match func {
        Func::Root | Func::Log | Func::Round => arity(args, 1, 2)?,
        _ => arity(args, 1, 1)?,
    }
    let first = args[0];
    match func {
        Func::Sqrt => root_of(first, 2, scope),
        Func::Cbrt => root_of(first, 3, scope),
        Func::Root => nth_root(args),
        Func::Abs => keep_unit(first, |n| n.abs()),
        Func::Log => log(args),
        Func::Ln => plain(first, f64::ln),
        Func::Fact => factorial(first),
        Func::Round => round(args),
        Func::Ceil => keep_unit(first, |n| n.map_integer(|d: Decimal| d.ceil(), f64::ceil)),
        Func::Floor => keep_unit(first, |n| n.map_integer(|d: Decimal| d.floor(), f64::floor)),
        Func::Sin => trig(first, f64::sin, scope),
        Func::Cos => trig(first, f64::cos, scope),
        Func::Tan => trig(first, f64::tan, scope),
        Func::Asin => inverse_trig(first, f64::asin),
        Func::Acos => inverse_trig(first, f64::acos),
        Func::Atan => inverse_trig(first, f64::atan),
        Func::Sinh => plain(first, f64::sinh),
        Func::Cosh => plain(first, f64::cosh),
        Func::Tanh => plain(first, f64::tanh),
        Func::FromUnix => dates::from_unix(first, scope.ctx),
        Func::Custom(_) => fail("unreachable"),
    }
}

fn plain(value: Value, f: fn(f64) -> f64) -> Eval<Value> {
    Ok(Value::plain(clean(f(plain_arg(value)?))?))
}

fn keep_unit(value: Value, f: impl Fn(Num) -> Num) -> Eval<Value> {
    let q = quantity(value)?;
    Ok(Value::Q(q.map(f(q.n))))
}

/// `sqrt`/`cbrt` of a plain number, or of a square/cubic length (an
/// extension of Numi, which drops the unit).
fn root_of(value: Value, degree: u8, scope: &Scope<'_>) -> Eval<Value> {
    let q = quantity(value)?;
    let x = q.n.to_f64();
    let r = if degree == 2 { x.sqrt() } else { x.cbrt() };
    match q.unit {
        Unit::Phys(u) if scope.vocab.is_length_family(u) && u.power == degree => Ok(Value::Q(
            Quantity::with(clean(r)?, Unit::Phys(u.with_power(1))),
        )),
        _ => Ok(Value::plain(clean(r)?)),
    }
}

fn nth_root(args: &[Value]) -> Eval<Value> {
    let [degree, x] = args else {
        return fail("root needs a degree and a number");
    };
    let (n, x) = (plain_arg(*degree)?, plain_arg(*x)?);
    let odd = n.fract() == 0.0 && (n as i64) % 2 != 0;
    let r = if x < 0.0 && odd {
        -(-x).powf(1.0 / n)
    } else {
        x.powf(1.0 / n)
    };
    Ok(Value::plain(clean(r)?))
}

fn log(args: &[Value]) -> Eval<Value> {
    let result = match args {
        [x] => plain_arg(*x)?.log10(),
        [base, x] => plain_arg(*x)?.log2() / plain_arg(*base)?.log2(),
        _ => return fail("log needs one or two arguments"),
    };
    Ok(Value::plain(clean(result)?))
}

/// Γ(x + 1): exact for whole numbers, Lanczos otherwise.
fn factorial(value: Value) -> Eval<Value> {
    let x = plain_arg(value)?;
    if x > MAX_FACTORIAL {
        return fail("factorial too large");
    }
    if x.fract() == 0.0 && x >= 0.0 {
        let n = x as i64;
        return Ok(Value::plain(
            (2..=n).fold(Num::ONE, |acc, k| acc.mul(Num::int(k))),
        ));
    }
    Ok(Value::plain(clean(gamma(x + 1.0))?))
}

/// Lanczos approximation (g = 7, n = 9), about 15 significant digits.
fn gamma(x: f64) -> f64 {
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    use std::f64::consts::PI;
    if x < 0.5 {
        return PI / ((PI * x).sin() * gamma(1.0 - x));
    }
    let x = x - 1.0;
    let t = x + G + 0.5;
    let sum = C
        .iter()
        .enumerate()
        .skip(1)
        .fold(C[0], |acc, (i, c)| acc + c / (x + i as f64));
    (2.0 * PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * sum
}

fn round(args: &[Value]) -> Eval<Value> {
    let digits = match args.get(1) {
        Some(d) => u32::try_from(plain_arg(*d)? as i64).unwrap_or(0),
        None => 0,
    };
    keep_unit(args[0], |n| match n {
        Num::Dec(d) => {
            Num::Dec(d.round_dp_with_strategy(digits, RoundingStrategy::MidpointAwayFromZero))
        }
        Num::Float(_) => n.round_dp(digits),
    })
}

/// `sin`/`cos`/`tan`: unitless means degrees.
fn trig(value: Value, f: fn(f64) -> f64, scope: &Scope<'_>) -> Eval<Value> {
    let q = quantity(value)?;
    let (Some(radian), Some(degree)) = (
        unit_ref(scope.vocab, "radian"),
        unit_ref(scope.vocab, "degree"),
    ) else {
        return fail("no angle units");
    };
    let from = match q.unit {
        Unit::Phys(u) if scope.vocab.dim(u) == Dim::Angle => u,
        _ => degree,
    };
    let radians = scope
        .vocab
        .convert(q.n, from, radian, scope.env)
        .unwrap_or(q.n);
    let result = f(radians.to_f64());
    // sin(180°) is 1.2e-16 in binary floating point; show it as 0.
    let result = if result.abs() < 1e-15 { 0.0 } else { result };
    Ok(Value::plain(clean(result)?))
}

/// `arcsin`/`arccos`/`arctan`, in degrees.
fn inverse_trig(value: Value, f: fn(f64) -> f64) -> Eval<Value> {
    let degrees = f(plain_arg(value)?).to_degrees();
    let rounded = (degrees * 1e12).round() / 1e12;
    Ok(Value::plain(clean(rounded)?))
}
