//! Table-driven behaviour tests against the public API, one table per
//! section of `docs/numi-spec.md`. Each case is a document and the
//! expected result column of its last line: the display text, `ERR` for an
//! error line, or `` for a line without a result.

mod common;

use common::{check, ctx, engine, shown};
use numero_engine::{Config, Engine, LineKind, TokenClass};

#[test]
fn arithmetic() {
    check(&[
        ("1 + 2", "3"),
        ("0.1 + 0.2", "0.3"),
        ("10 / 4", "2.5"),
        ("2 ^ 10", "1,024"),
        ("-(3 + 4)", "-7"),
        ("10 mod 3", "1"),
        ("-7 mod 3", "-1"),
        ("1/3", "0.33"),
        ("2/3", "0.67"),
        ("1000000 * 1000000", "1,000,000,000,000"),
        ("8 / 0", "ERR"),
        ("3 x 4", "12"),
        ("3x4", "12"),
        ("3 × 4 ÷ 2", "6"),
        ("3 ✕ 4", "12"),
        ("2**3", "8"),
        ("2ˆ3", "8"),
        ("10 − 3", "7"),
        ("10 — 3", "7"),
        ("(2+3", "5"),
    ]);
}

/// §2.2-2.3: priorities 100/200/300, leftmost first, unary minus tightest.
#[test]
fn precedence() {
    check(&[
        ("2 * 3 + 4", "10"),
        ("2 + 3 * 4", "14"),
        ("(2 + 3) * 4", "20"),
        ("2^3^2", "64"),
        ("-2^2", "4"),
        ("2 * -3", "-6"),
        ("2 * 3 ^ 2", "18"),
        ("6 / 2 * 3", "9"),
        // §2.3 writes this as `5 & 4`, but its own priorities (& 200 > + 100)
        // bind `&` first.
        ("5 & 3 + 1", "2"),
        ("1 + 5 & 3", "2"),
        ("1 << 4 + 1", "17"),
        ("1 + 2 m in cm", "201 cm"),
        ("(1 + 2 m) in cm", "300 cm"),
        ("5!", "5"),
        ("2(3 + 4)", ""),
        ("(1 + 2)(3 + 4)", ""),
        ("2 pi", ""),
        ("1 + 5% of 100", "ERR"),
    ]);
}

#[test]
fn word_operators() {
    check(&[
        ("3 plus 4", "7"),
        ("5 and 6", "11"),
        ("5 with 2", "7"),
        ("10 minus 3", "7"),
        ("10 without 3", "7"),
        ("10 subtract 3", "7"),
        ("6 times 7", "42"),
        ("3 multiplied by 4", "12"),
        ("4 mul 5", "20"),
        ("4 multiply 5", "20"),
        ("12 divided by 4", "3"),
        ("12 divide by 4", "3"),
        ("12 divide 4", "3"),
    ]);
}

#[test]
fn bitwise() {
    check(&[
        ("10 & 6", "2"),
        ("10 | 6", "14"),
        ("10 xor 6", "12"),
        ("1 << 4", "16"),
        ("256 >> 4", "16"),
        ("5.7 & 3", "1"),
    ]);
}

/// §4.1: grouping with spaces, underscores and commas.
#[test]
fn number_literals() {
    check(&[
        ("1,000 + 1", "1,001"),
        ("1 000 + 1", "1,001"),
        ("1_000 * 2", "2,000"),
        ("5 3", "53"),
        (".5 + 1", "1.5"),
        ("1,234.5", "1,234.5"),
        (
            "123456789012345678901234",
            "123,456,789,012,345,678,901,234",
        ),
        ("0.000001", "0"),
    ]);
}

/// §2.1, §5: free text, comments, labels, headers.
#[test]
fn free_text_comments_labels() {
    check(&[
        ("$9 for coffee + $3 tip", "$12"),
        ("hello world", ""),
        ("// just a note", ""),
        ("5 + 5 // ten", "10"),
        ("\"quoted 5\" 10 + 1", "11"),
        ("10 + \"five\" 1", "11"),
        ("5 apples + 3 apples", "8"),
        ("Label: 5 + 5", "10"),
        ("Rent: $500", "$500"),
        ("Item 2: 5", "5"),
        ("a: b: 7", "7"),
        ("Meeting: 2:30 pm", "2:30 PM"),
        ("# Header", ""),
        ("   # Indented header", ""),
        ("I need a car", ""),
        ("in the morning", ""),
    ]);
}

/// §1.5, §5: variables and constants.
#[test]
fn variables_and_constants() {
    check(&[
        ("v = 20\nv * 2", "40"),
        ("v is 20\nv + 1", "21"),
        ("v equal 20\nv", "20"),
        ("v = 20", "20"),
        ("price = $10\nprice * 3", "$30"),
        ("a = 5\nb = 6\na * b", "30"),
        ("x = 3\nx^2", "9"),
        ("x = 3\n2 x", ""),
        ("v = 2\nv = v + 1\nv", "3"),
        ("pi", "3.14"),
        ("π * 2", "6.28"),
        ("e", "2.72"),
        ("r", "8.31"),
        ("2 * pi", "6.28"),
        ("ppi", "96"),
        ("pi = 3\npi * 2", "6"),
        ("unknown * 2", ""),
    ]);
}

/// §5: `prev`, `sum`, `avg`.
#[test]
fn prev_sum_and_average() {
    check(&[
        ("10\nprev * 2", "20"),
        ("10\nhello\nprev + 1", "ERR"),
        ("prev", "ERR"),
        ("10\n20\n30\nsum", "60"),
        ("10\n20\n30\ntotal", "60"),
        ("10\n20\n30\navg", "20"),
        ("10\n20\n30\naverage", "20"),
        ("10\n\n20\n30\nsum", "50"),
        ("10\n# Section\n20\n30\nsum", "50"),
        ("10\nhello\n20\n30\nsum", "50"),
        ("10\n5 km in kg\n20\nsum", "20"),
        ("$10\n$20\nsum", "$30"),
        ("1 km\n500 m\nsum", "1.5 km"),
        ("500 m\n1 km\nsum", "1,500 m"),
        ("sum", "0"),
        ("10\n20\nsum * 2", "60"),
        ("10\n20\nsum\nsum", "60"),
        ("1 km\n5 kg\nsum", "ERR"),
        ("10\ntoday\n20\nsum", "ERR"),
        ("10\n20\navg + 1", "16"),
    ]);
}

/// §4.3: scales.
#[test]
fn scales() {
    check(&[
        ("$2k", "$2,000"),
        ("3 million", "3,000,000"),
        ("2 billion", "2,000,000,000"),
        ("2 milliard", "2,000,000,000"),
        ("5M", "5,000,000"),
        ("1.5k", "1,500"),
        ("2 thousand", "2,000"),
        ("2k + 500", "2,500"),
        ("5k USD", "$5,000"),
        ("3 million EUR", "€ 3,000,000"),
        ("5 m", "5 m"),
    ]);
}

/// §3: the twelve percent operations.
#[test]
fn percentages() {
    check(&[
        ("20% of 10", "2"),
        ("20% from 10", "2"),
        ("5% on 30", "31.5"),
        ("6% off 40", "37.6"),
        ("5% of what is 6", "120"),
        ("5% on what is 6", "5.71"),
        ("5% off what is 6", "6.32"),
        ("50 as a % of 100", "50 %"),
        ("130 as a % on 100", "30 %"),
        ("70 as a % off 100", "30 %"),
        ("70 as a % on 20", "250 %"),
        ("20 as a % off 70", "71.43 %"),
        ("50 as a % increase of 40", "25 %"),
        ("30 as a percent decrease of 40", "25 %"),
        ("50 as a percent of 200", "25 %"),
        ("50 cm as a % of 1 m", "50 %"),
        ("100 + 20%", "120"),
        ("100 - 20%", "80"),
        ("50 * 20%", "10"),
        ("20% * 50", "10"),
        ("100 / 20%", "5"),
        ("8 / (45 - 20%)", "0.22"),
        ("10%", "10 %"),
        ("20 percent of 50", "10"),
        ("$100 + 15%", "$115"),
        ("10% + 5%", "15 %"),
        ("10% + 5", "15 %"),
        ("0.25 in %", "0.25 %"),
    ]);
}

/// §1.2-1.3: length, area, volume with Numi's ratios.
#[test]
fn length_area_volume() {
    check(&[
        ("5 inches in cm", "12.7 cm"),
        ("1 km in m", "1,000 m"),
        ("1 mile in km", "1.61 km"),
        ("3 feet in inches", "36 ″"),
        ("1 yard in feet", "3 ft"),
        ("1 league in miles", "3 mi."),
        ("1 nautical mile in m", "1,853.18 m"),
        ("1 cable in m", "185.32 m"),
        ("1 point in mm", "0.35 mm"),
        ("72 pt in inches", "1 ″"),
        ("1 kilometer in meters", "1,000 m"),
        ("5 millimeters in cm", "0.5 cm"),
        ("5 m + 3 ft", "5.91 m"),
        ("1 m + 20 cm", "1.2 m"),
        ("20 cm + 1 m", "120 cm"),
        ("5 + 3 cm", "8 cm"),
        ("2 km * 3", "6 km"),
        ("3 * 2 km", "6 km"),
        ("10 km / 2 km", "5"),
        ("10 km / 2", "5 km"),
        ("10 km / 2 h", "5 km"),
        ("100 / 4 m", "25"),
        ("3 m * 4 m", "12 m²"),
        ("12 m² / 3 m", "4 m"),
        ("1 hectare in m²", "10,000 m²"),
        ("1 acre in square meters", "4,046.86 m²"),
        ("1 acre in hectares", "0.4 ha"),
        ("1 sq km in hectares", "100 ha"),
        ("1 km2 in m2", "1,000,000 m²"),
        ("1 ping in m2", "3.31 m²"),
        ("1 square foot in square inches", "144 ″²"),
        ("1 sq mi in acres", "640 acre"),
        ("1 m³ in liters", "1,000 L"),
        ("1 cubic meter in litres", "1,000 L"),
        ("5 cm3 in ml", "5 mL"),
        ("1 cc in ml", "1 mL"),
        ("1 cu inch in cm3", "16.39 cm³"),
        ("2 cups in ml", "473.18 mL"),
        ("1 gallon in litres", "3.79 L"),
        ("1 tbsp in tsp", "3 tsp."),
        ("1 quart in pints", "2 pint"),
        ("5 km in kg", "ERR"),
        ("5 km + 3 kg", "ERR"),
        ("3 m 20 cm", ""),
        ("5 km 3", "5 km³"),
    ]);
}

#[test]
fn mass_time_angle_temperature() {
    check(&[
        ("1 kg in pounds", "2.2 lb"),
        ("1 lb in g", "453.59 g"),
        ("1 stone in kg", "6.35 kg"),
        ("1 tonne in kg", "1,000 kg"),
        ("1 kt in tonnes", "1,000 t"),
        ("1 oz in g", "28.35 g"),
        ("1 carat in g", "0.2 g"),
        ("1 centner in kg", "100 kg"),
        ("1 hour in minutes", "60 min"),
        ("1 day in hours", "24 h"),
        ("1 week in days", "7 day"),
        ("90 min in hours", "1.5 h"),
        ("1000 ms in s", "1 s"),
        ("1 µs in ns", "1,000 ns"),
        ("1 μs in ns", "1,000 ns"),
        ("1 year in days", "365 day"),
        ("1 month in days", "30 day"),
        ("1 year in months", "12.17 mon."),
        ("180 degrees in radians", "3.14 rad"),
        ("1 rad in degrees", "57.3 °"),
        ("90°", "90 °"),
        ("100 °C in °F", "212 °F"),
        ("32 F in C", "0 °C"),
        ("0 K in celsius", "-273.15 °C"),
        ("20 celsius in kelvin", "293.15 K"),
        ("-40 C in F", "-40 °F"),
    ]);
}

/// §1.2 data and CSS, §1.4 prefixes.
#[test]
fn data_and_css() {
    check(&[
        ("1 GB in MB", "1,000 MB"),
        ("1 GiB in MiB", "1,024 MiB"),
        ("1 byte in bits", "8 b"),
        ("8 bits in bytes", "1 B"),
        ("1 kB in bytes", "1,000 B"),
        ("1 KB in B", "1,000 B"),
        ("2 megabytes in kilobytes", "2,000 kB"),
        ("1 kibibyte in bytes", "1,024 B"),
        ("1 Mb in kB", "125 kB"),
        ("5 mb", "5"),
        ("12 pt in px", "16 px"),
        ("1 inch in px", "96 px"),
        ("2 em in px", "32 px"),
        ("ppi = 326\n1 inch in px", "326 px"),
        ("ppi = 72\n1 inch in px", "72 px"),
        ("em = 20px\n2 em in px", "40 px"),
        ("32 px in em", "2 em"),
    ]);
}

/// §4.1-4.2: numeral systems in and out.
#[test]
fn number_formats() {
    check(&[
        ("255 in hex", "0xff"),
        ("0xFF", "0xff"),
        ("0b1010", "0b1010"),
        ("0o17", "0o17"),
        ("0xFF in decimal", "255"),
        ("0xFF in dec", "255"),
        ("0xFF + 1", "0x100"),
        ("1 + 0xFF", "256"),
        ("10 in binary", "0b1010"),
        ("10 in bin", "0b1010"),
        ("8 in octal", "0o10"),
        ("1234 in sci", "1.23E3"),
        ("1234 in scientific", "1.23E3"),
        ("1234 in exp", "1.23E3"),
        ("255 in hex + 1", "0x100"),
        ("-255 in hex", "-0xff"),
        ("255.9 in hex", "0xff"),
        ("1e3", "1E3"),
        ("1e3 in decimal", "1,000"),
        ("2.5e-3", "2.5E-3"),
    ]);
}

/// §1.2 currency, §4.4 placement, §7 currency formatting.
#[test]
fn currencies() {
    check(&[
        ("$9 in EUR", "€ 8.10"),
        ("$9 in Euro", "€ 8.10"),
        ("10 EUR in USD", "$11.11"),
        ("€10 + $10", "€ 19"),
        ("$10 + €9", "$20"),
        ("100 JPY in USD", "$0.67"),
        ("¥150 in dollars", "$1"),
        ("1 BTC in USD", "$100,000"),
        ("1 bitcoin in dollars", "$100,000"),
        ("2 ETH in $", "$5,000"),
        ("£5 in dollars", "$6.25"),
        ("10 dollars", "$10"),
        ("5 euro", "€ 5"),
        ("5$", "$5"),
        ("USD 5", "$5"),
        ("€5", "€ 5"),
        ("$5 * 3", "$15"),
        ("$5 * $3", "$15"),
        ("20 USD / 4", "$5"),
        ("$20 / $4", "5"),
        ("10 CHF in USD", "ERR"),
        ("5 Swiss francs", "5 SFr."),
        ("$1,000 in EUR", "€ 900"),
        ("$1.5", "$1.50"),
        ("$1.25", "$1.25"),
        ("-$5", "-$5"),
        ("$5 - $8", "-$3"),
        ("$0.1 + $0.2", "$0.30"),
        ("10 RUB", "10 ₽"),
    ]);
}

#[test]
fn currencies_without_rates_have_no_result() {
    let engine = Engine::new(Config::default());
    let results = engine.evaluate_with("$9 in Euro\n$9 + $1", &ctx());
    assert_eq!(results[0].kind, LineKind::Error);
    assert_eq!(results[0].display, None);
    assert_eq!(results[1].display.as_deref(), Some("$10"));
}

/// §9: functions.
#[test]
fn functions() {
    check(&[
        ("sqrt(16)", "4"),
        ("sqrt 16", "4"),
        ("sqrt 16 + 9", "13"),
        ("square root 16", "4"),
        ("cbrt(27)", "3"),
        ("cube root 27", "3"),
        ("root 3 (27)", "3"),
        ("root(3; 27)", "3"),
        ("root 3 (-27)", "-3"),
        ("abs(-5)", "5"),
        ("log(100)", "2"),
        ("log 100", "2"),
        ("log 2 (8)", "3"),
        ("log(2; 8)", "3"),
        ("log 1 (5)", "ERR"),
        ("ln(e)", "1"),
        ("ln 0", "ERR"),
        ("fact(5)", "120"),
        ("fact 0", "1"),
        ("fact 0.5", "0.89"),
        ("round(3.6)", "4"),
        ("round(2.5)", "3"),
        ("round(-2.5)", "-3"),
        ("ceil(3.2)", "4"),
        ("floor(3.8)", "3"),
        ("sin(30)", "0.5"),
        ("sin 30", "0.5"),
        ("sin(30°)", "0.5"),
        ("sin(pi rad)", "0"),
        ("cos(0)", "1"),
        ("cos 60", "0.5"),
        ("tan(45)", "1"),
        ("arcsin(1)", "90"),
        ("arccos(1)", "0"),
        ("arctan(1)", "45"),
        ("sinh(0)", "0"),
        ("cosh(0)", "1"),
        ("tanh(0)", "0"),
        ("sqrt(-4)", "ERR"),
        ("sqrt(16) + 0.1", "4.1"),
        ("sqrt(16 m²)", "4 m"),
        ("abs(-5 km)", "5 km"),
        ("2 sqrt(9)", ""),
        ("fromunix(0)", "1/1/70, 1:00 AM"),
        ("unixtime(86400)", "1/2/70, 1:00 AM"),
    ]);
}

/// §8: dates and time zones, en_US short styles.
#[test]
fn dates_and_times() {
    check(&[
        ("today", "10/14/26"),
        ("tomorrow", "10/15/26"),
        ("yesterday", "10/13/26"),
        ("today + 3 weeks", "11/4/26"),
        ("today + 1 month", "11/13/26"),
        ("today - 1 year", "10/14/25"),
        ("today + 3 days", "10/17/26"),
        ("tomorrow - yesterday", "172,800 s"),
        ("(tomorrow - yesterday) in days", "2 day"),
        ("now", "2:00 PM"),
        ("time", "2:00 PM"),
        ("now + 2 hours", "4:00 PM"),
        ("now + 1 day", "10/15/26, 2:00 PM"),
        ("PST time", "5:00 AM"),
        ("pst time", "5:00 AM"),
        ("MST time", "5:00 AM"),
        ("Berlin time", "2:00 PM"),
        ("time in New York", "8:00 AM"),
        ("now in Tokyo", "9:00 PM"),
        ("2:30 pm HKT in Berlin", "8:30 AM"),
        ("14:30 in UTC", "12:30 PM"),
        ("3pm in London", "2:00 PM"),
        ("12:00", "12:00 PM"),
        ("12 am", "12:00 AM"),
        ("time in local", "2:00 PM"),
        ("time in Sao Paulo", "9:00 AM"),
        ("NYC time", "8:00 AM"),
        ("time in Mumbai", "5:30 PM"),
        ("time in GMT+3", "3:00 PM"),
        ("now in gmt -5", "7:00 AM"),
        ("now - today", "0 s"),
        ("today + 5 km", "ERR"),
    ]);
}

#[test]
fn every_line_gets_a_result() {
    let doc = "1\n\n# h\n// c\nfoo\n2 +\n$1 in XYZ";
    let results = engine().evaluate_with(doc, &ctx());
    assert_eq!(results.len(), 7);
    let kinds: Vec<LineKind> = results.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds[..5],
        [
            LineKind::Value,
            LineKind::Empty,
            LineKind::Header,
            LineKind::Comment,
            LineKind::Comment
        ]
    );
}

#[test]
fn spans_are_ordered_disjoint_and_in_bounds() {
    let doc = "Label: $9 for coffee + 20% of 5 km in miles // note\nv = sqrt(16) + 0xFF\n# Head\n\"q\" 2:30 pm HKT in Berlin\nκόσμε 5 €\n1 000 + GMT+3";
    let results = engine().evaluate_with(doc, &ctx());
    for (line, result) in doc.split('\n').zip(&results) {
        let mut end = 0;
        for span in &result.spans {
            assert!(span.range.start >= end, "{line:?}: overlapping {span:?}");
            assert!(
                span.range.end <= line.len() && span.range.start < span.range.end,
                "{line:?}: {span:?}"
            );
            assert!(
                line.is_char_boundary(span.range.start) && line.is_char_boundary(span.range.end)
            );
            end = span.range.end;
        }
    }
}

#[test]
fn highlight_classes() {
    let line = "Label: v = $9 in EUR + sqrt(4) // hi";
    let results = engine().evaluate_with(&format!("v = 1\n{line}"), &ctx());
    let classes: Vec<(&str, TokenClass)> = results[1]
        .spans
        .iter()
        .map(|s| (&line[s.range.clone()], s.class))
        .collect();
    let expect = [
        ("Label:", TokenClass::Label),
        ("v", TokenClass::Variable),
        ("=", TokenClass::Operator),
        ("$", TokenClass::Currency),
        ("9", TokenClass::Number),
        ("in", TokenClass::Keyword),
        ("EUR", TokenClass::Currency),
        ("+", TokenClass::Operator),
        ("sqrt", TokenClass::Function),
        ("(", TokenClass::Operator),
        ("4", TokenClass::Number),
        (")", TokenClass::Operator),
        ("// hi", TokenClass::Comment),
    ];
    assert_eq!(classes, expect);
}

#[test]
fn header_and_unit_spans() {
    let results = engine().evaluate_with("# Sample\n5 inches in cm", &ctx());
    assert_eq!(results[0].spans[0].class, TokenClass::Header);
    let classes: Vec<TokenClass> = results[1].spans.iter().map(|s| s.class).collect();
    assert_eq!(
        classes,
        [
            TokenClass::Number,
            TokenClass::Unit,
            TokenClass::Keyword,
            TokenClass::Unit
        ]
    );
}

#[test]
fn numbers_are_exposed() {
    let results = engine().evaluate_with("5 inches in cm\ntoday", &ctx());
    assert_eq!(results[0].number, Some(12.7));
    assert_eq!(results[1].number, None);
}

#[test]
fn sample_document() {
    let results = engine().evaluate_with(
        "# Sample\n8 / (45 - 20%)\n5 inches in cm\n$9 in Euro",
        &ctx(),
    );
    let shown: Vec<String> = results.iter().map(shown).collect();
    assert_eq!(shown, ["", "0.22", "12.7 cm", "€ 8.10"]);
}

/// §7: precision and separators come from the config.
#[test]
fn precision_and_separators_follow_config() {
    let mut engine = engine();
    engine.set_config(Config {
        precision: 4,
        grouping_separator: None,
        decimal_separator: ',',
        ..Config::default()
    });
    let results = engine.evaluate_with("1/3\n1234567,5\n1.234.567,5", &ctx());
    let shown: Vec<String> = results.iter().map(shown).collect();
    assert_eq!(shown, ["0,3333", "1234567,5", "1234567,5"]);
    engine.set_config(Config {
        precision: 0,
        ..Config::default()
    });
    assert_eq!(
        engine.evaluate_with("2.5", &ctx())[0].display.as_deref(),
        Some("3")
    );
}

/// §5: the bottom-bar grand total.
#[test]
fn grand_total() {
    let engine = engine();
    let evaluation = engine.evaluate_document("10\n20\nhello\n\n$5", &ctx());
    assert_eq!(evaluation.grand_total.as_deref(), Some("$35"));
    let evaluation = engine.evaluate_document("1 km\n500 m\n2 kg", &ctx());
    assert_eq!(evaluation.grand_total.as_deref(), Some("1.5 km"));
    assert_eq!(evaluation.grand_total_number, Some(1.5));
    assert_eq!(engine.evaluate_document("hello", &ctx()).grand_total, None);
}

/// §1.1: other locales' phrases, English merged in.
#[test]
fn locales() {
    assert_eq!(Engine::locales().len(), 28);
    let mut engine = engine();
    engine.set_locale("de").expect("German tables");
    common::check_with(
        &engine,
        &[
            ("5 Meter in cm", "500 cm"),
            ("heute", "10/14/26"),
            ("5 meters in cm", "500 cm"),
        ],
    );
    assert!(engine.set_locale("xx").is_err());
    assert_eq!(engine.locale(), "de");
}

/// §12.11: completion hints from the phrase tables.
#[test]
fn hints() {
    let hints = engine().hints("kilom", 5);
    assert!(!hints.is_empty() && hints.len() <= 5);
    assert!(
        hints.iter().all(|h| h.to_lowercase().starts_with("kilom")),
        "{hints:?}"
    );
    assert!(engine().hints("", 5).is_empty());
}

#[test]
fn thousand_lines_evaluate_quickly() {
    Engine::warm_up();
    let lines = [
        "price = $1,250",
        "price * 3 + 15% // markup",
        "5 inches in cm",
        "20% of 10 km in miles",
        "sqrt(16) + 2^10",
        "today + 3 weeks",
        "Groceries: $45 for food + $12 tip",
        "sum",
        "",
        "# Section",
    ];
    let doc: Vec<&str> = lines.iter().cycle().take(1000).copied().collect();
    let doc = doc.join("\n");
    let engine = engine();
    let ctx = ctx();
    let started = std::time::Instant::now();
    let results = engine.evaluate_with(&doc, &ctx);
    let elapsed = started.elapsed();
    assert_eq!(results.len(), 1000);
    eprintln!("1,000 lines evaluated in {elapsed:?}");
    // Release builds take about a millisecond; debug builds are far slower.
    let budget = if cfg!(debug_assertions) { 400 } else { 10 };
    assert!(elapsed.as_millis() < budget, "1,000 lines took {elapsed:?}");
}
