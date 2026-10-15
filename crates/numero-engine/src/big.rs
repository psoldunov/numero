//! Decimals beyond `rust_decimal`'s 28 digits.
//!
//! Numi computes with `NSDecimalNumber`: a 128-bit mantissa (38 or 39
//! decimal digits) and a base-10 exponent (§0). Integer powers and long
//! literals stay exact there (`10^38 + 1`, `2^100`, a 30-digit number), so
//! [`Big`] keeps the same shape: `±mantissa × 10^exponent` with a `u128`
//! mantissa, rounded half away from zero whenever a result needs more.
//! [`crate::num::Num`] only reaches for it when a `Decimal` overflows.

use rust_decimal::Decimal;

/// Significant digits a mantissa always holds (`10^38 < 2^128`).
const DIGITS: u32 = 38;
/// `10^38`, the scale a quotient is computed at.
const LIMIT: u128 = 10u128.pow(DIGITS);
/// Exponent range of `NSDecimal`.
const MIN_EXPONENT: i32 = -128;
const MAX_EXPONENT: i32 = 127;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Big {
    negative: bool,
    mantissa: u128,
    exponent: i32,
}

/// A 256-bit unsigned integer as two halves, for products and dividends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wide {
    hi: u128,
    lo: u128,
}

impl Wide {
    fn from(value: u128) -> Self {
        Self { hi: 0, lo: value }
    }

    /// The full product of two `u128`s.
    fn product(a: u128, b: u128) -> Self {
        const MASK: u128 = u64::MAX as u128;
        let (a_hi, a_lo) = (a >> 64, a & MASK);
        let (b_hi, b_lo) = (b >> 64, b & MASK);
        let low = a_lo * b_lo;
        let mid1 = a_hi * b_lo;
        let mid2 = a_lo * b_hi;
        let high = a_hi * b_hi;
        let (mid, mid_carry) = mid1.overflowing_add(mid2);
        let (lo, lo_carry) = low.overflowing_add(mid << 64);
        let hi = high + (mid >> 64) + (u128::from(mid_carry) << 64) + u128::from(lo_carry);
        Self { hi, lo }
    }

    /// Divide by a small number; returns the quotient and remainder.
    fn div_small(self, divisor: u64) -> (Self, u64) {
        let d = u128::from(divisor);
        let hi = self.hi / d;
        let mut rem = self.hi % d;
        let mut lo = 0u128;
        for shift in [64u32, 0] {
            let chunk = (rem << 64) | ((self.lo >> shift) & u128::from(u64::MAX));
            lo |= (chunk / d) << shift;
            rem = chunk % d;
        }
        (Self { hi, lo }, u64::try_from(rem).unwrap_or(0))
    }

    /// Long division by a non-zero `u128`.
    fn div_wide(self, divisor: u128) -> Self {
        let mut quotient = Self { hi: 0, lo: 0 };
        let mut rem = 0u128;
        for bit in (0..256u32).rev() {
            let word = if bit >= 128 { self.hi } else { self.lo };
            let carry = rem >> 127;
            rem = (rem << 1) | ((word >> (bit % 128)) & 1);
            if carry == 1 || rem >= divisor {
                rem = rem.wrapping_sub(divisor);
                if bit >= 128 {
                    quotient.hi |= 1 << (bit - 128);
                } else {
                    quotient.lo |= 1 << bit;
                }
            }
        }
        quotient
    }

    fn add(self, other: Self) -> Self {
        let (lo, carry) = self.lo.overflowing_add(other.lo);
        Self {
            hi: self.hi + other.hi + u128::from(carry),
            lo,
        }
    }
}

impl Big {
    /// `±mantissa × 10^exponent` from a wide mantissa, rounded to fit a
    /// `u128`. `None` outside `NSDecimal`'s exponent range.
    fn from_wide(negative: bool, mut mantissa: Wide, mut exponent: i32) -> Option<Self> {
        let mut dropped = 0u64;
        while mantissa.hi != 0 {
            (mantissa, dropped) = mantissa.div_small(10);
            exponent += 1;
        }
        let mut mantissa = mantissa.lo;
        if dropped >= 5 {
            mantissa = match mantissa.checked_add(1) {
                Some(m) => m,
                None => {
                    exponent += 1;
                    mantissa / 10 + 1
                }
            };
        }
        Self::normal(negative, mantissa, exponent)
    }

    /// Strip trailing zeros and check the exponent range.
    fn normal(negative: bool, mut mantissa: u128, mut exponent: i32) -> Option<Self> {
        if mantissa == 0 {
            return Some(Self::ZERO);
        }
        while mantissa.is_multiple_of(10) {
            mantissa /= 10;
            exponent += 1;
        }
        while exponent < MIN_EXPONENT && mantissa != 0 {
            let rest = mantissa % 10;
            mantissa = mantissa / 10 + u128::from(rest >= 5);
            exponent += 1;
        }
        if mantissa == 0 {
            return Some(Self::ZERO);
        }
        (exponent <= MAX_EXPONENT).then_some(Self {
            negative,
            mantissa,
            exponent,
        })
    }

    pub const ZERO: Big = Big {
        negative: false,
        mantissa: 0,
        exponent: 0,
    };

    /// `±mantissa × 10^exponent`.
    pub fn new(negative: bool, mantissa: u128, exponent: i32) -> Option<Self> {
        Self::from_wide(negative, Wide::from(mantissa), exponent)
    }

    pub fn from_decimal(d: Decimal) -> Self {
        let scale = i32::try_from(d.scale()).unwrap_or(0);
        Self::new(d.is_sign_negative(), d.mantissa().unsigned_abs(), -scale).unwrap_or(Self::ZERO)
    }

    /// The same value as a `Decimal`, if it fits one exactly.
    pub fn to_decimal(self) -> Option<Decimal> {
        let mut mantissa = i128::try_from(self.mantissa).ok()?;
        let mut scale = 0u32;
        if self.exponent >= 0 {
            for _ in 0..self.exponent {
                mantissa = mantissa.checked_mul(10)?;
            }
        } else {
            scale = self.exponent.unsigned_abs();
        }
        if self.negative {
            mantissa = -mantissa;
        }
        Decimal::try_from_i128_with_scale(mantissa, scale).ok()
    }

    /// A literal of plain digits with an optional point (`1234.5`), any
    /// length; digits beyond what a `u128` holds are rounded off.
    pub fn parse(text: &str) -> Option<Self> {
        let (int, frac) = text.split_once('.').unwrap_or((text, ""));
        if int.is_empty() && frac.is_empty()
            || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let digits: String = format!("{int}{frac}");
        let digits = digits.trim_start_matches('0');
        let exponent = -i32::try_from(frac.len()).ok()?;
        if digits.is_empty() {
            return Some(Self::ZERO);
        }
        let mut keep = digits.len().min(DIGITS as usize + 1);
        let mantissa = loop {
            match digits[..keep].parse::<u128>() {
                Ok(m) => break m,
                Err(_) => keep -= 1,
            }
        };
        let dropped = i32::try_from(digits.len() - keep).ok()?;
        let round_up = digits.as_bytes().get(keep).is_some_and(|b| *b >= b'5');
        let mantissa = Wide::from(mantissa).add(Wide::from(u128::from(round_up)));
        Self::from_wide(false, mantissa, exponent + dropped)
    }

    pub fn to_f64(self) -> f64 {
        let sign = if self.negative { "-" } else { "" };
        format!("{sign}{}e{}", self.mantissa, self.exponent)
            .parse()
            .unwrap_or(f64::NAN)
    }

    pub fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    pub fn is_negative(self) -> bool {
        self.negative && self.mantissa != 0
    }

    pub fn is_integer(self) -> bool {
        self.exponent >= 0 || self.mantissa == 0
    }

    pub fn neg(self) -> Self {
        Self {
            negative: !self.negative && self.mantissa != 0,
            ..self
        }
    }

    pub fn abs(self) -> Self {
        Self {
            negative: false,
            ..self
        }
    }

    pub fn add(self, other: Self) -> Option<Self> {
        if self.is_zero() {
            return Some(other);
        }
        if other.is_zero() {
            return Some(self);
        }
        // Bring both to the smaller exponent where that stays exact;
        // otherwise round the smaller-exponent operand off first.
        let (mut a, mut b) = (self, other);
        if a.exponent < b.exponent {
            std::mem::swap(&mut a, &mut b);
        }
        let mut a_m = a.mantissa;
        let mut a_e = a.exponent;
        while a_e > b.exponent && a_m <= u128::MAX / 10 {
            a_m *= 10;
            a_e -= 1;
        }
        let mut b_m = b.mantissa;
        let mut b_e = b.exponent;
        while b_e < a_e {
            let rest = b_m % 10;
            b_m = b_m / 10 + u128::from(rest >= 5);
            b_e += 1;
        }
        let (negative, mantissa) = match (a.negative, b.negative) {
            (x, y) if x == y => (x, Wide::from(a_m).add(Wide::from(b_m))),
            _ if a_m >= b_m => (a.negative, Wide::from(a_m - b_m)),
            _ => (b.negative, Wide::from(b_m - a_m)),
        };
        Self::from_wide(negative, mantissa, a_e)
    }

    pub fn sub(self, other: Self) -> Option<Self> {
        self.add(other.neg())
    }

    pub fn mul(self, other: Self) -> Option<Self> {
        Self::from_wide(
            self.negative != other.negative,
            Wide::product(self.mantissa, other.mantissa),
            self.exponent + other.exponent,
        )
    }

    /// Division to at least 38 significant digits; `None` for a zero
    /// divisor.
    pub fn div(self, other: Self) -> Option<Self> {
        if other.is_zero() {
            return None;
        }
        if self.is_zero() {
            return Some(Self::ZERO);
        }
        // Widen the dividend to 38 digits, then by 10^38 more, so the
        // quotient keeps at least 37 significant digits.
        let mut dividend = self.mantissa;
        let mut shift = 0i32;
        while dividend <= u128::MAX / 10 {
            dividend *= 10;
            shift += 1;
        }
        let quotient = Wide::product(dividend, LIMIT).div_wide(other.mantissa);
        Self::from_wide(
            self.negative != other.negative,
            quotient,
            self.exponent - shift - other.exponent - i32::try_from(DIGITS).unwrap_or(0),
        )
    }

    /// Round to `dp` fraction digits, halves away from zero.
    pub fn round_dp(self, dp: u32) -> Self {
        let target = -i32::try_from(dp).unwrap_or(i32::MAX);
        if self.exponent >= target {
            return self;
        }
        let drop = (target - self.exponent).unsigned_abs();
        if drop > DIGITS {
            return Self::ZERO;
        }
        let divisor = 10u128.pow(drop);
        let mut mantissa = self.mantissa / divisor;
        if self.mantissa % divisor >= divisor / 2 {
            mantissa += 1;
        }
        Self::normal(self.negative, mantissa, target).unwrap_or(Self::ZERO)
    }

    /// Drop the fraction (towards zero).
    pub fn trunc(self) -> Self {
        if self.is_integer() {
            return self;
        }
        let drop = self.exponent.unsigned_abs();
        if drop > DIGITS {
            return Self::ZERO;
        }
        Self::normal(self.negative, self.mantissa / 10u128.pow(drop), 0).unwrap_or(Self::ZERO)
    }

    pub fn floor(self) -> Self {
        let t = self.trunc();
        if self.is_negative() && t != self {
            t.sub(Self::new(false, 1, 0).unwrap_or(Self::ZERO))
                .unwrap_or(t)
        } else {
            t
        }
    }

    pub fn ceil(self) -> Self {
        let t = self.trunc();
        if !self.is_negative() && t != self {
            t.add(Self::new(false, 1, 0).unwrap_or(Self::ZERO))
                .unwrap_or(t)
        } else {
            t
        }
    }

    /// The value as an integer, if it is one and fits.
    pub fn to_i128(self) -> Option<i128> {
        if !self.is_integer() {
            return None;
        }
        let mut value = i128::try_from(self.mantissa).ok()?;
        for _ in 0..self.exponent {
            value = value.checked_mul(10)?;
        }
        Some(if self.negative { -value } else { value })
    }

    /// Plain digits with a point, no sign and no exponent: `1000`,
    /// `0.25`, `333.33`.
    pub fn digits(self) -> String {
        let m = self.mantissa.to_string();
        if self.exponent >= 0 {
            return format!("{m}{}", "0".repeat(self.exponent.unsigned_abs() as usize));
        }
        let frac = self.exponent.unsigned_abs() as usize;
        if m.len() > frac {
            let (int, rest) = m.split_at(m.len() - frac);
            format!("{int}.{rest}")
        } else {
            format!("0.{}{m}", "0".repeat(frac - m.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big(text: &str) -> Big {
        Big::parse(text).expect("literal")
    }

    #[test]
    fn long_literals_stay_exact() {
        let n = big("123456789012345678901234567890");
        assert_eq!(n.digits(), "123456789012345678901234567890");
        let one = big("1");
        assert_eq!(
            n.add(one).map(Big::digits).as_deref(),
            Some("123456789012345678901234567891")
        );
    }

    #[test]
    fn products_and_quotients_round_to_38_digits() {
        let nines = big("99999999999999999999999999999");
        let nine = big("9");
        assert_eq!(
            nines.mul(nine).map(Big::digits).as_deref(),
            Some("899999999999999999999999999991")
        );
        let third = big("1000000000000000000000000000000")
            .div(big("3"))
            .expect("div");
        assert_eq!(
            third.round_dp(2).digits(),
            "333333333333333333333333333333.33"
        );
        assert_eq!(
            big("2")
                .div(big("3"))
                .map(|b| b.round_dp(2).digits())
                .as_deref(),
            Some("0.67")
        );
    }

    #[test]
    fn wide_division_by_small_numbers() {
        let w = Wide::product(u128::MAX, 10);
        let (q, r) = w.div_small(10);
        assert_eq!((q, r), (Wide::from(u128::MAX), 0));
    }

    #[test]
    fn decimals_round_trip() {
        let d = Decimal::new(-12345, 2);
        assert_eq!(Big::from_decimal(d).to_decimal(), Some(d));
    }
}
