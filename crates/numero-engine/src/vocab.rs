//! The phrase tables, compiled into a [`Trie`] plus the unit, prefix and
//! currency tables the evaluator formats with.
//!
//! A locale is two `key = a,b,c` files under `data/<locale>/` generated
//! from Numi's `.strings`. English is always merged in underneath the
//! chosen locale (Numi does the same): the locale's phrases are inserted
//! first and win ties, its formats replace English ones. `extra.txt` and
//! `cities.tsv` are shared. Extensions add units and functions to an
//! engine's own copy (see [`Vocab::add_unit`]).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use crate::locales::LOCALES;
use crate::num::Num;
use crate::trie::Trie;
use crate::units::{PREFIXES, Prefixes, UnitDef, UnitRef, UnitTable, builtin_units, unit_index};
use crate::zones;

pub use crate::vocab_ext::UnitLike;

const EN_VOCAB: &str = include_str!("../data/en/vocab.txt");
const EN_CURRENCY: &str = include_str!("../data/en/currency.txt");
const EN_EXTRA: &str = include_str!("../data/en/extra.txt");

/// Phrases a unit symbol never generates because they mean something else.
/// `pm` (picometre) and `at` (attotonne) are not among them: Numi reads
/// `1 nm in pm` and `5 at` as units (§1.4 [oracle]).
const RESERVED: &[&str] = &["am", "as", "in", "is", "on", "of", "to", "mod", "min"];

/// Phrases in Numi's tables that its matcher never reaches: `25 °C` is
/// `25 °` (the angle) followed by a stray `C`, and `K` is the thousand
/// scale, not kelvin (§1.2 [oracle]).
const UNMATCHED: &[&str] = &["°C", "°F", "K"];

/// Month names (en_US calendar symbols): `5 oct`, `oct 5` (§8.1 [oracle]).
const MONTHS: [(&str, &str); 12] = [
    ("january", "jan"),
    ("february", "feb"),
    ("march", "mar"),
    ("april", "apr"),
    ("may", "may"),
    ("june", "jun"),
    ("july", "jul"),
    ("august", "aug"),
    ("september", "sept"),
    ("october", "oct"),
    ("november", "nov"),
    ("december", "dec"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func {
    Sqrt,
    Cbrt,
    Root,
    Abs,
    Log,
    Ln,
    Fact,
    Round,
    Ceil,
    Floor,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    FromUnix,
    /// An extension function, by index into the engine's function list.
    Custom(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumFmt {
    Decimal,
    Hex,
    Binary,
    Octal,
    Scientific,
}

/// Percent operators (§3): `5% of 30`, `5% on 30`, `5% off 30`,
/// `5 as a % of 30`, `5% of what is 30` and their `on`/`off` forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PctOp {
    Of,
    On,
    Off,
    AsOf,
    AsOn,
    AsOff,
    OfWhatIs,
    OnWhatIs,
    OffWhatIs,
}

impl PctOp {
    /// `of`/`on`/`off` and the `what is` forms need a percentage on the left.
    pub fn needs_percent_left(self) -> bool {
        !matches!(self, PctOp::AsOf | PctOp::AsOn | PctOp::AsOff)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Meaning {
    Unit(UnitRef),
    Currency(u16),
    Scale(Num),
    Percent,
    Pct(PctOp),
    Conversion,
    Assign,
    Plus,
    Minus,
    Times,
    Divide,
    Mod,
    Xor,
    Sum,
    Avg,
    Prev,
    Func(Func),
    Pi,
    E,
    Format(NumFmt),
    Today,
    Tomorrow,
    Yesterday,
    Now,
    Local,
    Zone(chrono_tz::Tz),
    /// A month of the year, 1 to 12.
    Month(u8),
    /// `GMT+3`, `gmt -5`: Numi's `gmt` function. Its offset is lost: as a
    /// conversion target it is GMT, after a time it is ignored, and on its
    /// own the line has no result (§8.2 [oracle]).
    GmtOffset,
}

/// Classes for de-duplicating phrases: one phrase may carry several
/// meanings of different classes (French `m` is both the thousand scale
/// and, through English, the metre; context picks), but two currencies
/// sharing "Kwacha" keep the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Unit,
    Currency,
    Scale,
    Zone,
    /// Months share `dec` with the decimal format word.
    Month,
    Other,
}

impl Meaning {
    pub fn kind(self) -> Kind {
        match self {
            Meaning::Unit(_) => Kind::Unit,
            Meaning::Currency(_) => Kind::Currency,
            Meaning::Scale(_) => Kind::Scale,
            Meaning::Zone(_) => Kind::Zone,
            Meaning::Month(_) => Kind::Month,
            _ => Kind::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Currency {
    pub code: String,
    pub format: String,
}

#[derive(Debug, Clone)]
pub struct Vocab {
    pub trie: Trie,
    pub units: Vec<UnitDef>,
    /// Output format per entry of `units`.
    pub unit_formats: Vec<String>,
    /// SI symbol per entry of `units` (`m`, `l`, `B`), or its format.
    pub unit_symbols: Vec<String>,
    /// SI/IEC symbol per entry of [`PREFIXES`].
    pub prefix_symbols: Vec<String>,
    pub area_format: String,
    pub volume_format: String,
    pub currencies: Vec<Currency>,
    pub currency_index: HashMap<String, u16>,
    /// Ids of extension functions; `Func::Custom(i)` is entry `i`.
    pub functions: Vec<String>,
}

impl UnitTable for Vocab {
    fn unit_def(&self, unit: UnitRef) -> &UnitDef {
        self.units.as_slice().unit_def(unit)
    }
}

static ENGLISH: LazyLock<Arc<Vocab>> =
    LazyLock::new(|| Arc::new(Vocab::build(&[(EN_VOCAB, EN_CURRENCY)])));
static OTHERS: LazyLock<Mutex<HashMap<&'static str, Arc<Vocab>>>> = LazyLock::new(Mutex::default);

pub fn english() -> Arc<Vocab> {
    Arc::clone(&ENGLISH)
}

/// Locale identifiers with phrase tables, English first.
pub fn locales() -> impl Iterator<Item = &'static str> {
    std::iter::once("en").chain(LOCALES.iter().map(|(name, _, _)| *name))
}

/// The tables for `locale` (`de`, `pt-BR`), English merged in; `None` for
/// an unknown locale. Built once per locale and shared.
pub fn for_locale(locale: &str) -> Option<Arc<Vocab>> {
    if locale == "en" {
        return Some(english());
    }
    let &(name, vocab, currency) = LOCALES.iter().find(|(name, _, _)| *name == locale)?;
    let mut cache = OTHERS.lock().unwrap_or_else(PoisonError::into_inner);
    let built = cache
        .entry(name)
        .or_insert_with(|| Arc::new(Vocab::build(&[(vocab, currency), (EN_VOCAB, EN_CURRENCY)])));
    Some(Arc::clone(built))
}

type Table<'a> = HashMap<&'a str, &'a str>;

/// `key = value` pairs of a table file, comments and section lines skipped.
fn ordered_pairs(text: &str) -> Vec<(&str, &str)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('['))
        .filter_map(|l| l.split_once(" = ").map(|(k, v)| (k.trim(), v.trim())))
        .collect()
}

fn pairs(text: &str) -> Table<'_> {
    ordered_pairs(text).into_iter().collect()
}

pub fn split_variants(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').map(str::trim).filter(|v| !v.is_empty())
}

/// The first table's value for `key`.
fn first<'a>(tables: &[Table<'a>], key: &str) -> Option<&'a str> {
    tables.iter().find_map(|t| t.get(key).copied())
}

/// Every variant for `key` across the tables, highest priority first.
fn all<'a>(tables: &[Table<'a>], key: &str) -> Vec<&'a str> {
    tables
        .iter()
        .filter_map(|t| t.get(key).copied())
        .flat_map(split_variants)
        .collect()
}

/// Short symbols are case-sensitive (`M` million, `m` metre, `B` byte,
/// `b` bit); so are all-caps codes (`USD`, `PST`). Words are not.
pub fn is_case_sensitive(phrase: &str) -> bool {
    let count = phrase.chars().count();
    let mut letters = phrase.chars().filter(|c| c.is_alphabetic()).peekable();
    let has_letters = letters.peek().is_some();
    count <= 2 || (count <= 5 && has_letters && letters.all(char::is_uppercase))
}

impl Vocab {
    /// Build from `(vocab, currency)` sources, highest priority first.
    pub fn build(sources: &[(&str, &str)]) -> Self {
        let tables: Vec<Table<'_>> = sources.iter().map(|(v, _)| pairs(v)).collect();
        let extra = pairs(EN_EXTRA);
        let mut this = Self {
            trie: Trie::default(),
            units: builtin_units(),
            unit_formats: Vec::new(),
            unit_symbols: Vec::new(),
            prefix_symbols: Vec::new(),
            area_format: first(&tables, "area.format").unwrap_or("%@²").to_string(),
            volume_format: first(&tables, "volume.format").unwrap_or("%@³").to_string(),
            currencies: Vec::new(),
            currency_index: HashMap::new(),
            functions: Vec::new(),
        };
        this.add_words(&tables, &extra);
        this.add_units(&tables);
        for (_, currency) in sources {
            this.add_currencies(&ordered_pairs(currency));
        }
        this.add_currencies(&ordered_pairs(EN_EXTRA));
        zones::add_zone_phrases(&mut this.trie);
        this
    }

    pub(crate) fn insert(&mut self, phrase: &str, meaning: Meaning) {
        if matches!(meaning, Meaning::Unit(_)) && UNMATCHED.contains(&phrase) {
            return;
        }
        self.trie.insert(phrase, is_case_sensitive(phrase), meaning);
    }

    fn insert_all(&mut self, variants: &[&str], meaning: Meaning) {
        for variant in variants {
            self.insert(variant, meaning);
        }
    }

    fn add_words(&mut self, tables: &[Table<'_>], extra: &Table<'_>) {
        use Meaning as M;
        let words: &[(&str, Meaning)] = &[
            ("as_a_percent_of.variants", M::Pct(PctOp::AsOf)),
            ("as_a_percent_on.variants", M::Pct(PctOp::AsOn)),
            ("as_a_percent_off.variants", M::Pct(PctOp::AsOff)),
            ("percent_of_what_is.variants", M::Pct(PctOp::OfWhatIs)),
            ("percent_on_what_is.variants", M::Pct(PctOp::OnWhatIs)),
            ("percent_off_what_is.variants", M::Pct(PctOp::OffWhatIs)),
            ("percent_of.variants", M::Pct(PctOp::Of)),
            ("percent_on.variants", M::Pct(PctOp::On)),
            ("percent_off.variants", M::Pct(PctOp::Off)),
            ("percent.variants", M::Percent),
            ("assignment.variants", M::Assign),
            ("conversion.variants", M::Conversion),
            ("plus.variants", M::Plus),
            ("minus.variants", M::Minus),
            ("multiply.variants", M::Times),
            ("divide.variants", M::Divide),
            ("sum.variants", M::Sum),
            ("avg.variants", M::Avg),
            ("prev.variants", M::Prev),
            ("thousand.variants", M::Scale(Num::int(1_000))),
            ("million.variants", M::Scale(Num::int(1_000_000))),
            ("billion.variants", M::Scale(Num::int(1_000_000_000))),
            ("hex.variants", M::Format(NumFmt::Hex)),
            ("binary.variants", M::Format(NumFmt::Binary)),
            ("octal.variants", M::Format(NumFmt::Octal)),
            ("decimal.variants", M::Format(NumFmt::Decimal)),
            ("scientific.variants", M::Format(NumFmt::Scientific)),
            ("today.variants", M::Today),
            ("tomorrow.variants", M::Tomorrow),
            ("yesterday.variants", M::Yesterday),
            ("local_time.variants", M::Local),
            ("current_time.variants", M::Now),
            ("sqrt.variants", M::Func(Func::Sqrt)),
            ("cbrt.variants", M::Func(Func::Cbrt)),
        ];
        for (key, meaning) in words {
            self.insert_all(&all(tables, key), *meaning);
        }
        let extras: &[(&str, Meaning)] = &[
            ("fn.sqrt.variants", M::Func(Func::Sqrt)),
            ("fn.cbrt.variants", M::Func(Func::Cbrt)),
            ("fn.root.variants", M::Func(Func::Root)),
            ("fn.abs.variants", M::Func(Func::Abs)),
            ("fn.log.variants", M::Func(Func::Log)),
            ("fn.ln.variants", M::Func(Func::Ln)),
            ("fn.fact.variants", M::Func(Func::Fact)),
            ("fn.round.variants", M::Func(Func::Round)),
            ("fn.ceil.variants", M::Func(Func::Ceil)),
            ("fn.floor.variants", M::Func(Func::Floor)),
            ("fn.sin.variants", M::Func(Func::Sin)),
            ("fn.cos.variants", M::Func(Func::Cos)),
            ("fn.tan.variants", M::Func(Func::Tan)),
            ("fn.asin.variants", M::Func(Func::Asin)),
            ("fn.acos.variants", M::Func(Func::Acos)),
            ("fn.atan.variants", M::Func(Func::Atan)),
            ("fn.sinh.variants", M::Func(Func::Sinh)),
            ("fn.cosh.variants", M::Func(Func::Cosh)),
            ("fn.tanh.variants", M::Func(Func::Tanh)),
            ("fn.fromunix.variants", M::Func(Func::FromUnix)),
            ("const.pi.variants", M::Pi),
            ("const.e.variants", M::E),
            ("mod.variants", M::Mod),
            ("xor.variants", M::Xor),
        ];
        for (key, meaning) in extras {
            let variants: Vec<&str> = extra
                .get(key)
                .copied()
                .map(split_variants)
                .into_iter()
                .flatten()
                .collect();
            self.insert_all(&variants, *meaning);
        }
        for (month, (long, short)) in (1u8..).zip(MONTHS) {
            self.insert_all(&[long, short, &long[..3]], M::Month(month));
        }
    }

    fn add_units(&mut self, tables: &[Table<'_>]) {
        self.prefix_symbols = PREFIXES
            .iter()
            .map(|p| {
                first(tables, &format!("{}.symbol", p.id))
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        self.unit_formats = self
            .units
            .iter()
            .map(|u| {
                first(tables, &format!("{}.format", u.id))
                    .unwrap_or(&u.id)
                    .to_string()
            })
            .collect();
        self.unit_symbols = self
            .units
            .iter()
            .zip(&self.unit_formats)
            .map(|(u, f)| {
                first(tables, &format!("{}.symbol", u.id))
                    .unwrap_or(f)
                    .to_string()
            })
            .collect();
        for index in 0..self.units.len() {
            let variants = all(tables, &format!("{}.variants", self.units[index].id));
            self.insert_all(&variants, Meaning::Unit(UnitRef::plain(index)));
        }
        // Cubic units the strings name outright (`cu inch`, `cc`); Numi
        // 3.35 never wires them up, Numero maps them to in³ and cm³.
        self.alias_cubic(tables, "cbinch", "inch", None);
        self.alias_cubic(
            tables,
            "cbcm",
            "meter",
            PREFIXES.iter().position(|p| p.id == "centi"),
        );
        self.add_prefixed_units(tables);
        self.add_power_templates(tables);
    }

    fn alias_cubic(&mut self, tables: &[Table<'_>], id: &str, base: &str, prefix: Option<usize>) {
        let Some(base) = unit_index(&self.units, base) else {
            return;
        };
        let unit = UnitRef {
            prefix: prefix.and_then(|p| u8::try_from(p).ok()),
            power: 3,
            ..UnitRef::plain(base)
        };
        self.insert_all(&all(tables, &format!("{id}.variants")), Meaning::Unit(unit));
    }

    pub(crate) fn prefix_allowed(def: &UnitDef, prefix: &crate::units::Prefix) -> bool {
        match def.prefixes {
            Prefixes::None => false,
            Prefixes::Si => !prefix.binary,
            Prefixes::Data => !prefix.negative,
        }
    }

    /// `km`, `kilometre`, `MiB`, `mebibytes`, `ml`, `millilitres`…
    fn add_prefixed_units(&mut self, tables: &[Table<'_>]) {
        let mut generated: Vec<(String, bool, Meaning)> = Vec::new();
        for (index, def) in self.units.iter().enumerate() {
            let variants = all(tables, &format!("{}.variants", def.id));
            let symbols: Vec<&str> = variants
                .iter()
                .copied()
                .filter(|v| v.chars().count() == 1)
                .collect();
            let words: Vec<&str> = variants
                .iter()
                .copied()
                .filter(|v| v.chars().count() > 2 && v.chars().all(char::is_alphabetic))
                .collect();
            for (p, prefix) in PREFIXES
                .iter()
                .enumerate()
                .filter(|(_, p)| Self::prefix_allowed(def, p))
            {
                let unit = UnitRef {
                    prefix: u8::try_from(p).ok(),
                    ..UnitRef::plain(index)
                };
                let mut prefix_symbols = vec![self.prefix_symbols[p].clone()];
                if prefix.id == "micro" {
                    prefix_symbols.push("μ".into());
                }
                for ps in prefix_symbols.iter().filter(|s| !s.is_empty()) {
                    for symbol in &symbols {
                        generated.push((format!("{ps}{symbol}"), true, Meaning::Unit(unit)));
                    }
                }
                for word in &words {
                    generated.push((
                        format!("{}{}", prefix.id, word.to_lowercase()),
                        false,
                        Meaning::Unit(unit),
                    ));
                }
            }
        }
        for (phrase, sensitive, meaning) in generated {
            if !RESERVED.contains(&phrase.as_str()) && !self.trie.contains(&phrase) {
                self.trie.insert(&phrase, sensitive, meaning);
            }
        }
    }

    /// `square meter`, `m²`, `sq ft`, `cubic inch`, `cm³`, `m3` (§1.3): on
    /// metres (all prefixes), inches, feet, yards and miles only.
    fn add_power_templates(&mut self, tables: &[Table<'_>]) {
        let area: Vec<String> = all(tables, "area.templates")
            .into_iter()
            .map(String::from)
            .chain(["%@2".to_string()])
            .collect();
        let volume: Vec<String> = all(tables, "volume.templates")
            .into_iter()
            .map(String::from)
            .chain(["%@3".to_string()])
            .collect();
        for (phrase, sensitive, unit) in self.length_phrases(tables) {
            for (templates, power) in [(&area, 2u8), (&volume, 3u8)] {
                for template in templates {
                    let text = template.replace("%@", &phrase);
                    let sensitive = sensitive && !template.chars().any(char::is_alphabetic);
                    let unit = UnitRef {
                        superscript: template.contains(['²', '³']),
                        ..unit.with_power(power)
                    };
                    if !self.trie.contains(&text) {
                        self.trie.insert(&text, sensitive, Meaning::Unit(unit));
                    }
                }
            }
        }
    }

    fn length_phrases(&self, tables: &[Table<'_>]) -> Vec<(String, bool, UnitRef)> {
        let mut out = Vec::new();
        for (index, def) in self.units.iter().enumerate().filter(|(_, d)| d.powers) {
            let variants = all(tables, &format!("{}.variants", def.id));
            for variant in &variants {
                out.push((
                    variant.to_string(),
                    is_case_sensitive(variant),
                    UnitRef::plain(index),
                ));
            }
            for (p, prefix) in PREFIXES
                .iter()
                .enumerate()
                .filter(|(_, p)| Self::prefix_allowed(def, p))
            {
                let unit = UnitRef {
                    prefix: u8::try_from(p).ok(),
                    ..UnitRef::plain(index)
                };
                for variant in &variants {
                    if variant.chars().count() == 1 {
                        out.push((format!("{}{variant}", self.prefix_symbols[p]), true, unit));
                    } else if variant.chars().all(char::is_alphabetic) {
                        out.push((
                            format!("{}{}", prefix.id, variant.to_lowercase()),
                            false,
                            unit,
                        ));
                    }
                }
            }
        }
        out
    }

    fn add_currencies(&mut self, entries: &[(&str, &str)]) {
        for (key, value) in entries {
            let Some((code, field)) = key.split_once('.') else {
                continue;
            };
            if !code.chars().all(|c| c.is_ascii_uppercase()) || code.len() < 3 {
                continue;
            }
            let (index, fresh) = self.currency(code);
            match field {
                "format" if fresh || self.currencies[usize::from(index)].format.is_empty() => {
                    self.currencies[usize::from(index)].format = (*value).to_string();
                }
                "variants" => {
                    for variant in split_variants(value) {
                        self.insert(variant, Meaning::Currency(index));
                    }
                }
                _ => {}
            }
        }
    }

    /// The index of a currency code, registering it if new.
    fn currency(&mut self, code: &str) -> (u16, bool) {
        if let Some(&index) = self.currency_index.get(code) {
            return (index, self.currencies[usize::from(index)].format.is_empty());
        }
        let index = u16::try_from(self.currencies.len()).unwrap_or(u16::MAX);
        self.currencies.push(Currency {
            code: code.to_string(),
            format: String::new(),
        });
        self.currency_index.insert(code.to_string(), index);
        self.trie.insert(code, true, Meaning::Currency(index));
        // Codes also match in lower case (`5 usd`), unless that spelling
        // already means something else (`cup`) (§1.2 [oracle]).
        let lower = code.to_lowercase();
        if !self.trie.contains(&lower) {
            self.trie.insert(&lower, true, Meaning::Currency(index));
        }
        (index, true)
    }

    /// Display template for a currency: its format, or `%@ CODE`.
    pub fn currency_format(&self, index: u16) -> String {
        match self.currencies.get(usize::from(index)) {
            Some(c) if !c.format.is_empty() => c.format.clone(),
            Some(c) => format!("%@ {}", c.code),
            None => "%@".into(),
        }
    }

    /// Display text for a unit: `km`, `m²`, `GiB`, `°C`. A prefix goes in
    /// front of the unit's symbol rather than its format, so the litre
    /// shows `L` but the millilitre `ml` (§1.2 [oracle]).
    pub fn unit_format(&self, unit: UnitRef) -> String {
        let index = usize::from(unit.def);
        let prefixed = match unit.prefix {
            Some(p) => format!(
                "{}{}",
                self.prefix_symbols
                    .get(usize::from(p))
                    .cloned()
                    .unwrap_or_default(),
                self.unit_symbols.get(index).cloned().unwrap_or_default()
            ),
            None => self.unit_formats.get(index).cloned().unwrap_or_default(),
        };
        match unit.power {
            2 => self.area_format.replace("%@", &prefixed),
            3 => self.volume_format.replace("%@", &prefixed),
            _ => prefixed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meaning(phrase: &str) -> Option<Meaning> {
        let vocab = english();
        let m = vocab.trie.longest_match(phrase, 0)?;
        (m.end == phrase.len()).then(|| m.meanings[0])
    }

    #[test]
    fn short_symbols_respect_case() {
        assert!(matches!(meaning("m"), Some(Meaning::Unit(_))));
        assert!(matches!(meaning("M"), Some(Meaning::Scale(_))));
        assert!(matches!(meaning("MB"), Some(Meaning::Unit(_))));
        assert!(matches!(meaning("Mb"), Some(Meaning::Unit(_))));
    }

    #[test]
    fn data_units_take_no_negative_prefixes() {
        assert!(meaning("mb").is_none());
        assert!(meaning("mB").is_none());
        assert!(matches!(meaning("kB"), Some(Meaning::Unit(_))));
    }

    #[test]
    fn long_phrases_ignore_case() {
        assert!(matches!(meaning("Nautical Miles"), Some(Meaning::Unit(_))));
        assert!(matches!(meaning("euro"), Some(Meaning::Currency(_))));
        assert!(matches!(
            meaning("As a % of"),
            Some(Meaning::Pct(PctOp::AsOf))
        ));
    }

    #[test]
    fn templates_build_square_and_cubic_units() {
        let Some(Meaning::Unit(u)) = meaning("km²") else {
            panic!("km² unknown")
        };
        assert_eq!(u.power, 2);
        let Some(Meaning::Unit(u)) = meaning("cubic inches") else {
            panic!("cubic inches unknown")
        };
        assert_eq!(u.power, 3);
        assert_eq!(english().unit_format(u), "″³");
        assert!(meaning("square chain").is_none());
    }

    #[test]
    fn explicit_variants_beat_generated_prefixes() {
        let Some(Meaning::Unit(u)) = meaning("pt") else {
            panic!("pt unknown")
        };
        assert_eq!(english().unit_def(u).id, "point");
    }

    #[test]
    fn unit_ids_round_trip() {
        let vocab = english();
        for id in [
            "m", "km", "m2", "km3", "foot2", "inch", "GiB", "kB", "ms", "litre", "ml",
        ] {
            let Some(UnitLike::Unit(unit)) = vocab.unit_by_id(id) else {
                panic!("{id} unknown")
            };
            assert_eq!(vocab.unit_id(unit), id, "{id}");
        }
        assert_eq!(
            vocab
                .unit_by_id("USD")
                .map(|u| matches!(u, UnitLike::Currency(_))),
            Some(true)
        );
    }

    #[test]
    fn locales_merge_english() {
        let de = for_locale("de").expect("German tables");
        let m = de.trie.longest_match("heute", 0).expect("heute");
        assert_eq!(m.meanings[0], Meaning::Today);
        assert!(de.trie.longest_match("today", 0).is_some());
        assert!(for_locale("xx").is_none());
        assert_eq!(locales().count(), 28);
    }
}
