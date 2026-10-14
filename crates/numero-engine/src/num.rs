//! Numbers: decimal first, binary floating point as the fallback.
//!
//! Numi computes with `NSDecimalNumber`, so `0.1 + 0.2` shows `0.3` and
//! `5 inches in cm` is exactly `12.7`. [`Num::Dec`] reproduces that with
//! `rust_decimal` (96-bit mantissa, 28 fraction digits) for `+ − × ÷`,
//! integer powers and unit ratios, which are all exact decimals. Anything a
//! decimal cannot hold falls back to [`Num::Float`]: transcendental functions
//! (`sin`, `ln`, `sqrt`), non-integer powers, angle ratios involving π,
//! exchange rates, and results beyond ±7.9 × 10²⁸. Once a value is a float it
//! stays one; the formatter rounds floats to 15 significant digits so their
//! binary noise never reaches the screen.

use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::{Decimal, RoundingStrategy};

/// Largest exponent computed by repeated decimal multiplication.
const MAX_EXACT_POWER: i64 = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Dec(Decimal),
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
        match exact {
            Some(d) => Some(Num::Dec(d)),
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
        Decimal::from_i128(value).map_or(Num::Float(value as f64), Num::Dec)
    }

    pub fn to_f64(self) -> f64 {
        match self {
            Num::Dec(d) => d.to_f64().unwrap_or(f64::NAN),
            Num::Float(f) => f,
        }
    }

    pub fn is_finite(self) -> bool {
        match self {
            Num::Dec(_) => true,
            Num::Float(f) => f.is_finite(),
        }
    }

    pub fn is_zero(self) -> bool {
        match self {
            Num::Dec(d) => d.is_zero(),
            Num::Float(f) => f == 0.0,
        }
    }

    pub fn is_negative(self) -> bool {
        match self {
            Num::Dec(d) => d.is_sign_negative() && !d.is_zero(),
            Num::Float(f) => f < 0.0,
        }
    }

    pub fn is_integer(self) -> bool {
        match self {
            Num::Dec(d) => d.fract().is_zero(),
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
            Num::Float(f) => Num::Float(-f),
        }
    }

    pub fn abs(self) -> Self {
        match self {
            Num::Dec(d) => Num::Dec(d.abs()),
            Num::Float(f) => Num::Float(f.abs()),
        }
    }

    pub fn add(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_add, |a, b| a + b)
    }

    pub fn sub(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_sub, |a, b| a - b)
    }

    pub fn mul(self, other: Num) -> Self {
        self.combine(other, Decimal::checked_mul, |a, b| a * b)
    }

    /// Division; `None` when dividing by zero.
    pub fn div(self, other: Num) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        Some(self.combine(other, Decimal::checked_div, |a, b| a / b))
    }

    /// Remainder with the sign of the dividend; `None` for a zero divisor.
    pub fn rem(self, other: Num) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        Some(self.combine(other, Decimal::checked_rem, |a, b| a % b))
    }

    pub fn pow(self, exponent: Num) -> Self {
        if let (Num::Dec(base), Some(e)) = (self, exponent.to_i64())
            && e.abs() <= MAX_EXACT_POWER
            && let Some(result) = decimal_powi(base, e)
        {
            return Num::Dec(result);
        }
        Num::Float(self.to_f64().powf(exponent.to_f64()))
    }

    /// Round to `dp` fraction digits, halves away from zero.
    pub fn round_dp(self, dp: u32) -> Self {
        match self {
            Num::Dec(d) => {
                Num::Dec(d.round_dp_with_strategy(dp, RoundingStrategy::MidpointAwayFromZero))
            }
            Num::Float(f) => {
                let scale = 10f64.powi(i32::try_from(dp).unwrap_or(i32::MAX));
                Num::Float((f * scale).round() / scale)
            }
        }
    }

    pub fn map_integer(self, op: fn(Decimal) -> Decimal, float_op: fn(f64) -> f64) -> Self {
        match self {
            Num::Dec(d) => Num::Dec(op(d)),
            Num::Float(f) => Num::Float(float_op(f)),
        }
    }

    /// The exact decimal behind this number, rounding floats to 15
    /// significant digits first; `None` when it does not fit a decimal.
    pub fn to_decimal(self) -> Option<Decimal> {
        match self {
            Num::Dec(d) => Some(d),
            Num::Float(f) if f.is_finite() && f.abs() < 7.0e27 => {
                Decimal::from_scientific(&format!("{f:.14e}")).ok()
            }
            Num::Float(_) => None,
        }
    }

    fn combine(
        self,
        other: Num,
        exact: fn(Decimal, Decimal) -> Option<Decimal>,
        float: fn(f64, f64) -> f64,
    ) -> Self {
        if let (Num::Dec(a), Num::Dec(b)) = (self, other)
            && let Some(result) = exact(a, b)
        {
            return Num::Dec(result);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_sums_are_exact() {
        let sum = Num::exact("0.1").add(Num::exact("0.2"));
        assert_eq!(sum, Num::exact("0.3"));
    }

    #[test]
    fn overflow_falls_back_to_float() {
        let big = Num::exact("1e27").mul(Num::exact("1e27"));
        assert!(matches!(big, Num::Float(_)));
        assert!((big.to_f64() - 1e54).abs() < 1e40);
    }

    #[test]
    fn integer_powers_stay_exact() {
        assert_eq!(Num::int(2).pow(Num::int(10)), Num::int(1024));
        assert_eq!(Num::int(2).pow(Num::int(-2)), Num::exact("0.25"));
        assert!(matches!(Num::int(2).pow(Num::exact("0.5")), Num::Float(_)));
    }

    #[test]
    fn division_by_zero_is_none() {
        assert_eq!(Num::ONE.div(Num::ZERO), None);
    }
}
