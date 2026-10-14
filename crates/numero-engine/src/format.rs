//! Number display (§7): at most `Config::precision` fraction digits,
//! trailing zeros trimmed, halves rounded up (away from zero), integer
//! digits grouped. Currency amounts with exactly one fraction digit get a
//! second (`$1.50`). There is no automatic scientific notation: tiny values
//! show as `0` and huge ones print every digit. `in sci` uses `1.23E5`,
//! `in hex` lowercase digits of the truncated integer.

use rust_decimal::RoundingStrategy;

use crate::Config;
use crate::num::Num;
use crate::vocab::NumFmt;

pub fn number(n: Num, config: &Config, currency: bool) -> String {
    let precision = u32::try_from(config.precision).unwrap_or(10);
    let text = match n.to_decimal() {
        Some(d) => d
            .round_dp_with_strategy(precision, RoundingStrategy::MidpointAwayFromZero)
            .normalize()
            .abs()
            .to_string(),
        None => float_text(n.to_f64().abs(), config.precision),
    };
    let (int, frac) = text.split_once('.').unwrap_or((&text, ""));
    let mut frac = frac.to_string();
    if currency && frac.len() == 1 {
        frac.push('0');
    }
    let mut out = String::with_capacity(text.len() + 8);
    if n.is_negative() && !(int.chars().all(|c| c == '0') && frac.chars().all(|c| c == '0')) {
        out.push('-');
    }
    out.push_str(&group(int, config.grouping_separator));
    if !frac.is_empty() {
        out.push(config.decimal_separator);
        out.push_str(&frac);
    }
    out
}

/// A float too large for a decimal, as plain digits with zeros trimmed.
fn float_text(f: f64, precision: usize) -> String {
    let text = format!("{f:.precision$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

fn group(int: &str, separator: Option<char>) -> String {
    let Some(sep) = separator else {
        return int.to_string();
    };
    let digits: Vec<char> = int.chars().collect();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(*c);
    }
    out
}

/// `1.23E5`: mantissa at `precision` fraction digits, zeros trimmed.
pub fn scientific(n: Num, config: &Config) -> String {
    let text = format!("{:.*e}", config.precision, n.to_f64());
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.trim_end_matches('0').trim_end_matches('.')
    } else {
        mantissa
    };
    format!(
        "{}E{exponent}",
        mantissa.replace('.', &config.decimal_separator.to_string())
    )
}

/// A number in the representation a line asked for (`in hex`, `in sci`).
pub fn with_format(n: Num, fmt: NumFmt, config: &Config, currency: bool) -> String {
    let radix = match fmt {
        NumFmt::Decimal => return number(n, config, currency),
        NumFmt::Scientific => return scientific(n, config),
        NumFmt::Hex => 16,
        NumFmt::Binary => 2,
        NumFmt::Octal => 8,
    };
    let truncated = n.map_integer(|d| d.trunc(), f64::trunc);
    let Some(int) = truncated.to_i128() else {
        return number(n, config, currency);
    };
    let sign = if int < 0 { "-" } else { "" };
    let magnitude = int.unsigned_abs();
    match radix {
        16 => format!("{sign}0x{magnitude:x}"),
        2 => format!("{sign}0b{magnitude:b}"),
        _ => format!("{sign}0o{magnitude:o}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(text: &str) -> String {
        number(Num::exact(text), &Config::default(), false)
    }

    #[test]
    fn rounds_trims_and_groups() {
        assert_eq!(show("1234567.891"), "1,234,567.89");
        assert_eq!(show("2.50"), "2.5");
        assert_eq!(show("0.005"), "0.01");
        assert_eq!(show("0.004"), "0");
        assert_eq!(show("-0.004"), "0");
        assert_eq!(show("-1.5"), "-1.5");
    }

    #[test]
    fn currency_gets_two_decimals_when_it_has_one() {
        let config = Config::default();
        assert_eq!(number(Num::exact("1.5"), &config, true), "1.50");
        assert_eq!(number(Num::exact("1.25"), &config, true), "1.25");
        assert_eq!(number(Num::exact("1"), &config, true), "1");
    }

    #[test]
    fn floats_lose_their_binary_noise() {
        let n = Num::Float(0.1 + 0.2);
        assert_eq!(
            number(
                n,
                &Config {
                    precision: 10,
                    ..Config::default()
                },
                false
            ),
            "0.3"
        );
    }

    #[test]
    fn huge_values_print_every_digit() {
        let shown = number(Num::Float(1.5e30), &Config::default(), false);
        assert_eq!(shown.len(), 41, "{shown}");
        assert!(
            shown.starts_with("1,") && !shown.contains(['e', 'E']),
            "{shown}"
        );
    }

    #[test]
    fn radix_formats() {
        let config = Config::default();
        assert_eq!(
            with_format(Num::int(255), NumFmt::Hex, &config, false),
            "0xff"
        );
        assert_eq!(
            with_format(Num::exact("255.9"), NumFmt::Hex, &config, false),
            "0xff"
        );
        assert_eq!(
            with_format(Num::int(10), NumFmt::Binary, &config, false),
            "0b1010"
        );
        assert_eq!(
            with_format(Num::int(8), NumFmt::Octal, &config, false),
            "0o10"
        );
        assert_eq!(
            with_format(Num::int(1234), NumFmt::Scientific, &config, false),
            "1.23E3"
        );
    }
}
