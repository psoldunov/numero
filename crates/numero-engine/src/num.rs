//! Numbers: decimal first, binary floating point as the last resort.
//!
//! Numi computes with `NSDecimalNumber`, so `0.1 + 0.2` shows `0.3` and
//! `5 inches in cm` is exactly `12.7`. [`Num::Dec`] reproduces that with
//! `rust_decimal` (96-bit mantissa, 28 fraction digits) for `+ − × ÷`,
//! integer powers and unit ratios, which are all exact decimals. Beyond
//! ±7.9 × 10²⁸ a value becomes a [`Num::Big`], Numi's own 38-digit decimal
//! shape, so `10^38` and 30-digit literals stay exact (§0 [oracle]).
//!
//! Transcendental functions, non-integer powers and π go through `f64`
//! in Numi too, and come back through Foundation's `Decimal(Double)`
//! conversion, which keeps at most 20 digits and invents low ones from the
//! double (`fact(25)` = 15,511,210,043,330,981,888,000,000).
//! [`Num::from_double`] emulates that conversion. [`Num::Float`] remains
//! for exchange rates and values neither decimal can hold.

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::{Decimal, RoundingStrategy};

use crate::big::Big;

/// Largest exponent computed by repeated decimal multiplication.
const MAX_EXACT_POWER: i64 = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Dec(Decimal),
    Big(Big),
    Float(f64),
}

impl Num {
    pub const ZERO: Num = Num::Dec(Decimal::ZERO);
    pub const ONE: Num = Num::Dec(Decimal::ONE);
    pub const HUNDRED: Num = Num::Dec(Decimal::ONE_HUNDRED);

    pub fn int(value: i64) -> Self {
        Num::Dec(Decimal::from(value))
    }

    /// A decimal literal such as `12.5`, `1e3` or `2.5E-4`.
    pub fn parse(text: &str) -> Option<Self> {
        let has_exponent = text.contains(['e', 'E']);
        let exact = if has_exponent {
            Decimal::from_scientific(text).ok()
        } else {
            Decimal::from_str_exact(text).ok()
        };
        if let Some(d) = exact {
            return Some(Num::Dec(d));
        }
        let big = match text.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => {
                let exponent: i32 = exponent.parse().ok()?;
                Big::parse(mantissa)
                    .and_then(|m| Big::new(false, 1, exponent).and_then(|scale| m.mul(scale)))
            }
            None => Big::parse(text),
        };
        match big {
            Some(b) => Some(Num::from_big(b)),
            None => text
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .map(Num::Float),
        }
    }

    /// An exact decimal from a table constant such as `"0.0254"`.
    pub fn exact(text: &str) -> Self {
        Self::parse(text).unwrap_or(Num::Float(f64::NAN))
    }

    pub fn from_i128(value: i128) -> Self {
        match Decimal::from_i128(value) {
            Some(d) => Num::Dec(d),
            None => Big::new(value < 0, value.unsigned_abs(), 0)
                .map_or(Num::Float(value as f64), Num::Big),
        }
    }

    /// A `Big` as a `Decimal` when it fits one.
    fn from_big(b: Big) -> Self {
        b.to_decimal().map_or(Num::Big(b), Num::Dec)
    }

    fn to_big(self) -> Option<Big> {
        match self {
            Num::Dec(d) => Some(Big::from_decimal(d)),
            Num::Big(b) => Some(b),
            Num::Float(_) => None,
        }
    }

    /// Foundation's `Decimal(Double)` (swift-foundation, also the old
    /// Swift overlay): scale the double by ten until it reaches 2⁶⁴, back
    /// one step, truncate to a `UInt64` mantissa. Every step rounds in
    /// binary, which is where Numi's invented digits come from. `None` for
    /// NaN, infinities and exponents outside `Int8`.
    pub fn from_double(value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        if value == 0.0 {
            return Some(Num::ZERO);
        }
        let limit = u64::MAX as f64;
        let mut val = value.abs();
        let mut exponent: i32 = 0;
        while val < limit {
            if exponent <= i32::from(i8::MIN) {
                return None;
            }
            val *= 10.0;
            exponent -= 1;
        }
        while limit <= val {
            if exponent >= i32::from(i8::MAX) {
                return None;
            }
            val /= 10.0;
            exponent += 1;
        }
        let mantissa = val as u64;
        Big::new(value < 0.0, u128::from(mantissa), exponent).map(Num::from_big)
    }

    pub fn to_f64(self) -> f64 {
        match self {
            Num::Dec(d) => d.to_f64().unwrap_or(f64::NAN),
            Num::Big(b) => b.to_f64(),
            Num::Float(f) => f,
        }
    }

    pub fn is_finite(self) -> bool {
        match self {
            Num::Dec(_) | Num::Big(_) => true,
            Num::Float(f) => f.is_finite(),
        }
    }

    pub fn is_zero(self) -> bool {
        match self {
            Num::Dec(d) => d.is_zero(),
            Num::Big(b) => b.is_zero(),
            Num::Float(f) => f == 0.0,
        }
    }

    pub fn is_negative(self) -> bool {
        match self {
            Num::Dec(d) => d.is_sign_negative() && !d.is_zero(),
            Num::Big(b) => b.is_negative(),
            Num::Float(f) => f < 0.0,
        }
    }

    pub fn is_integer(self) -> bool {
        match self {
            Num::Dec(d) => d.fract().is_zero(),
            Num::Big(b) => b.is_integer(),
            Num::Float(f) => f.is_finite() && f.fract() == 0.0,
        }
    }

    /// The value as an integer, if it is one and fits.
    pub fn to_i128(self) -> Option<i128> {
        if !self.is_integer() {
            return None;
        }
        match self {
            Num::Dec(d) => d.to_i128(),
            Num::Big(b) => b.to_i128(),
            Num::Float(f) if f.abs() < 1.0e30 => Some(f as i128),
            Num::Float(_) => None,
        }
    }

    pub fn to_i64(self) -> Option<i64> {
        self.to_i128().and_then(|i| i64::try_from(i).ok())
    }

    pub fn neg(self) -> Self {
        match self {
            Num::Dec(d) => Num::Dec(-d),
            Num::Big(b) => Num::Big(b.neg()),
            Num::Float(f) => Num::Float(-f),
        }
    }

    pub fn abs(self) -> Self {
        match self {
            Num::Dec(d) => Num::Dec(d.abs()),
            Num::Big(b) => Num::Big(b.abs()),
            Num::Float(f) => Num::Float(f.abs()),
        }
    }

    pub fn add(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_add, Big::add, |a, b| a + b)
    }

    pub fn sub(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_sub, Big::sub, |a, b| a - b)
    }

    pub fn mul(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_mul, Big::mul, |a, b| a * b)
    }

    /// Division; `None` when dividing by zero.
    pub fn div(self, other: Num) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        Some(self.combine(other, Decimal::checked_div, Big::div, |a, b| a / b))
    }

    /// Remainder with the sign of the dividend; `None` for a zero divisor.
    pub fn rem(self, other: Num) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        if let (Num::Dec(a), Num::Dec(b)) = (self, other)
            && let Some(r) = a.checked_rem(b)
        {
            return Some(Num::Dec(r));
        }
        let quotient = self.div(other)?.map_integer(|d| d.trunc(), f64::trunc);
        Some(self.sub(quotient.mul(other)))
    }

    pub fn pow(self, exponent: Num) -> Self {
        if let Some(e) = exponent.to_i64()
            && e.abs() <= MAX_EXACT_POWER
            && let Some(base) = self.to_big()
        {
            if let Num::Dec(d) = self
                && let Some(result) = decimal_powi(d, e)
            {
                return Num::Dec(result);
            }
            if let Some(result) = big_powi(base, e) {
                return Num::from_big(result);
            }
        }
        let f = self.to_f64().powf(exponent.to_f64());
        Num::from_double(f).unwrap_or(Num::Float(f))
    }

    /// Round to `dp` fraction digits, halves away from zero.
    pub fn round_dp(self, dp: u32) -> Self {
        match self {
            Num::Dec(d) => {
                Num::Dec(d.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero))
            }
            Num::Big(b) => Num::from_big(b.round_dp(dp)),
            Num::Float(f) => {
                let scale = 10f64.powi(i32::try_from(dp).unwrap_or(i32::MAX));
                Num::Float((f * scale).round() / scale)
            }
        }
    }

    /// Apply an integer rounding (`trunc`, `floor`, `ceil`). A `Big` takes
    /// the same rounding, recognised by what `op` does to ±0.5.
    pub fn map_integer(self, op: fn(Decimal) -> Decimal, float_op: fn(f64) -> f64) -> Self {
        match self {
            Num::Dec(d) => Num::Dec(op(d)),
            Num::Big(b) => {
                let half = Decimal::new(5, 1);
                let rounded = match (op(-half) < Decimal::ZERO, op(half) > Decimal::ZERO) {
                    (false, false) => b.trunc(),
                    (true, false) => b.floor(),
                    (false, true) => b.ceil(),
                    (true, true) => b.round_dp(0),
                };
                Num::from_big(rounded)
            }
            Num::Float(f) => Num::Float(float_op(f)),
        }
    }

    /// The exact decimal behind this number, rounding floats to 15
    /// significant digits first; `None` when it does not fit a decimal.
    pub fn to_decimal(self) -> Option<Decimal> {
        match self {
            Num::Dec(d) => Some(d),
            Num::Big(b) => b.to_decimal(),
            Num::Float(f) if f.is_finite() && f.abs() < 7.0e27 => {
                Decimal::from_scientific(&format!("{f:.14e}")).ok()
            }
            Num::Float(_) => None,
        }
    }

    /// The value as a [`Big`], rounding floats to 17 significant digits.
    pub fn to_big_lossy(self) -> Option<Big> {
        match self {
            Num::Float(f) if f.is_finite() => Num::parse(&format!("{f:.16e}"))?.to_big(),
            other => other.to_big(),
        }
    }

    fn combine(
        self,
        other: Num,
        exact: fn(Decimal, Decimal) -> Option<Decimal>,
        big: fn(Big, Big) -> Option<Big>,
        float: fn(f64, f64) -> f64,
    ) -> Self {
        if let (Num::Dec(a), Num::Dec(b)) = (self, other)
            && let Some(result) = exact(a, b)
        {
            return Num::Dec(result);
        }
        if let (Some(a), Some(b)) = (self.to_big(), other.to_big())
            && let Some(result) = big(a, b)
        {
            return Num::from_big(result);
        }
        Num::Float(float(self.to_f64(), other.to_f64()))
    }
}

fn decimal_powi(base: Decimal, exponent: i64) -> Option<Decimal> {
    let mut result = Decimal::ONE;
    let mut square = base;
    let mut remaining = exponent.unsigned_abs();
    while remaining > 0 {
        if remaining & 1 == 1 {
            result = result.checked_mul(square)?;
        }
        remaining >>= 1;
        if remaining > 0 {
            square = square.checked_mul(square)?;
        }
    }
    if exponent < 0 {
        Decimal::ONE.checked_div(result)
    } else {
        Some(result)
    }
}

fn big_powi(base: Big, exponent: i64) -> Option<Big> {
    let one = Big::new(false, 1, 0)?;
    let mut result = one;
    let mut square = base;
    let mut remaining = exponent.unsigned_abs();
    while remaining > 0 {
        if remaining & 1 == 1 {
            result = result.mul(square)?;
        }
        remaining >>= 1;
        if remaining > 0 {
            square = square.mul(square)?;
        }
    }
    if exponent < 0 {
        one.div(result)
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_sums_are_exact() {
        let sum = Num::exact("0.1").add(Num::exact("0.2"));
        assert_eq!(sum, Num::exact("0.3"));
    }

    #[test]
    fn overflow_moves_to_38_digit_decimals() {
        let big = Num::exact("1e27").mul(Num::exact("1e27"));
        assert!(matches!(big, Num::Big(_)));
        assert!((big.to_f64() - 1e54).abs() < 1e40);
        let power = Num::int(10).pow(Num::int(38));
        assert!(matches!(power, Num::Big(_)));
        assert_eq!(power.to_f64(), 1e38);
    }

    #[test]
    fn integer_powers_stay_exact() {
        assert_eq!(Num::int(2).pow(Num::int(10)), Num::int(1024));
        assert_eq!(Num::int(2).pow(Num::int(-2)), Num::exact("0.25"));
        assert!(!Num::int(2).pow(Num::exact("0.5")).is_integer());
    }

    #[test]
    fn division_by_zero_is_none() {
        assert_eq!(Num::ONE.div(Num::ZERO), None);
    }

    /// Values read off Numi 3.35 (oracle probes): `fact(25)`, `fact(22)`,
    /// `fact(30)` and `tan(90)` all come from `tgamma`/`tan` in `f64`.
    #[test]
    fn foundation_double_conversion_invents_numis_digits() {
        let digits = |n: Num| match n.to_big_lossy() {
            Some(b) => b.round_dp(2).digits(),
            None => String::new(),
        };
        let f25: f64 = "15511210043330985984000000".parse().expect("f64");
        assert_eq!(
            digits(Num::from_double(f25).expect("finite")),
            "15511210043330981888000000"
        );
        let f22: f64 = "1124000727777607680000".parse().expect("f64");
        assert_eq!(
            digits(Num::from_double(f22).expect("finite")),
            "1124000727777607680000"
        );
        let f30: f64 = "265252859812191058636308480000000".parse().expect("f64");
        assert_eq!(
            digits(Num::from_double(f30).expect("finite")),
            "265252859812191078400000000000000"
        );
        let tan90 = 90f64.to_radians().tan();
        assert_eq!(
            digits(Num::from_double(tan90).expect("finite")),
            "16331239353195368.45"
        );
        assert_eq!(Num::from_double(120.0), Some(Num::int(120)));
    }
}
