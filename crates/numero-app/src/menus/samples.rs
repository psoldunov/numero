//! The Help menu's calculations: a syntax guide and one sample per area,
//! in Numero's own words. Opening one adds it as a new calculation, which
//! the user may edit or delete like any other.

/// A sample calculation the Help menu offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sample {
    pub name: &'static str,
    pub text: &'static str,
}

/// What Help → Syntax Guide opens.
pub(crate) const SYNTAX_GUIDE: &str = "\
# Numero syntax guide
// Type on the left, read the answer on the right.
// A click on an answer copies it.

## Arithmetic
12 + 30 / 6
(3 + 4) * 2 ^ 3
17 mod 5

## Words work too
4 times 6 plus 2
100 divided by 8

## Units and currencies
3 ft in cm
65 mph in km/h
20 EUR in USD

## Percentages
15% of 80
200 + 10%
30 is what % of 120

## Variables
rate = 42 USD
rate * 8

## Totals
Rent: 1200
Food: 340
sum
// An empty line starts a new block for sum and average.

## Comments and labels
Groceries: 25 + 13 // labels end with a colon
\"quoted text is a comment too\"";

/// The samples, in the order the Help menu lists them.
pub(crate) const SAMPLES: [Sample; 7] = [
    Sample {
        name: "General",
        text: "\
# Weekend plans
Cinema: 2 * $14
Dinner: 63 EUR in USD
Taxi home: 18 + 15%
Bus pass: $25 - 10%
total",
    },
    Sample {
        name: "Conversion",
        text: "\
# Conversions
5 km in miles
180 cm in feet
2 cups in ml
350 °F in °C
1.5 TB in GB
90 minutes in hours",
    },
    Sample {
        name: "Sum",
        text: "\
# Groceries
Bread: 3.20
Cheese: 7.85
Apples: 4.10
sum

# Hardware store
Paint: 24
Brushes: 9.50
average",
    },
    Sample {
        name: "Dates",
        text: "\
# Dates and times
today
today + 3 weeks
tomorrow - 2 days
now in Tokyo
time in New York",
    },
    Sample {
        name: "Variables",
        text: "\
# Road trip
distance = 640 km
consumption = 6.5
fuel price = 1.85 EUR
litres = distance / 100 * consumption
litres * fuel price",
    },
    Sample {
        name: "Functions",
        text: "\
# Functions
sqrt(144)
round(7.65)
abs(-12)
sin(30)
log 10 (1000)
fact(6)",
    },
    Sample {
        name: "CSS",
        text: "\
# Design units
em = 18px
ppi = 132
2.5 em in px
24 pt in px
1 inch in px
320 px in em",
    },
];

#[cfg(test)]
mod tests {
    use numero_engine::{Config, Engine, LineKind, Rates};

    use super::*;

    fn errors(text: &str) -> Vec<(usize, &str)> {
        let mut engine = Engine::new(Config::default());
        let per_usd = [("USD", 1.), ("EUR", 0.92)]
            .into_iter()
            .map(|(code, rate)| (code.to_owned(), rate))
            .collect();
        engine.set_rates(Rates {
            per_usd,
            fetched_at: None,
        });
        let results = engine.evaluate(text);
        text.lines()
            .zip(results)
            .enumerate()
            .filter(|(_, (_, result))| result.kind == LineKind::Error)
            .map(|(index, (line, _))| (index, line))
            .collect()
    }

    #[test]
    fn every_sample_line_evaluates_without_an_error() {
        for sample in SAMPLES {
            assert_eq!(errors(sample.text), [], "sample {}", sample.name);
        }
        assert_eq!(errors(SYNTAX_GUIDE), []);
    }

    #[test]
    fn every_sample_starts_with_a_header() {
        for sample in SAMPLES {
            assert!(sample.text.starts_with("# "), "{}", sample.name);
        }
    }
}
