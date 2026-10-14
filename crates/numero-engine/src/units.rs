//! Unit definitions: dimensions and ratios to each dimension's base unit,
//! copied from Numi 3.35 (`docs/numi-spec.md` §1.2), including its odd
//! values (acre 4046.86 m², nautical mile 1853.184 m, 30-day month).
//! Locale-independent; phrases live in `data/<locale>/vocab.txt` keyed by
//! [`UnitDef::id`]. Extensions add units at runtime, so the table is a
//! `Vec` owned by the vocabulary rather than a `const`.

use crate::num::Num;

/// What a unit measures. Units convert only within a dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    Length,
    Area,
    Volume,
    Mass,
    Time,
    Angle,
    Temperature,
    Data,
}

/// How a unit relates to its dimension's base unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Factor {
    /// Base units per one of this unit.
    Ratio(Num),
    /// CSS pixel: `px = m × ppi / 0.0254`, with the document's `ppi`.
    Pixel,
    /// CSS em: the document's `em` in metres.
    Em,
}

/// Which metric prefixes a unit's symbol takes (§1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefixes {
    None,
    /// All SI prefixes, including milli, micro…
    Si,
    /// Positive SI prefixes and binary (IEC) ones, for data units.
    Data,
}

#[derive(Debug, Clone)]
pub struct UnitDef {
    /// Numi's unit id (`m`, `inch`, `gram`), also the locale-table key.
    pub id: String,
    pub dim: Dim,
    pub factor: Factor,
    /// Added before scaling to base (temperatures only).
    pub offset: Num,
    pub prefixes: Prefixes,
    /// Gets square and cubic forms from the area/volume templates.
    pub powers: bool,
}

struct Builtin {
    id: &'static str,
    dim: Dim,
    ratio: &'static str,
    offset: &'static str,
    prefixes: Prefixes,
    powers: bool,
}

const fn u(id: &'static str, dim: Dim, ratio: &'static str) -> Builtin {
    Builtin {
        id,
        dim,
        ratio,
        offset: "0",
        prefixes: Prefixes::None,
        powers: false,
    }
}

const fn si(id: &'static str, dim: Dim, ratio: &'static str, prefixes: Prefixes) -> Builtin {
    Builtin {
        id,
        dim,
        ratio,
        offset: "0",
        prefixes,
        powers: false,
    }
}

const fn sq(id: &'static str, ratio: &'static str) -> Builtin {
    Builtin {
        id,
        dim: Dim::Length,
        ratio,
        offset: "0",
        prefixes: Prefixes::None,
        powers: true,
    }
}

/// Base units: metre, square metre, cubic metre, gram, second, radian,
/// kelvin, bit. `pixel` and `em` are listed with ratio 1 and overridden by
/// [`Factor::Pixel`]/[`Factor::Em`].
const BUILTINS: &[Builtin] = &[
    Builtin {
        id: "meter",
        dim: Dim::Length,
        ratio: "1",
        offset: "0",
        prefixes: Prefixes::Si,
        powers: true,
    },
    sq("inch", "0.0254"),
    sq("foot", "0.3048"),
    sq("yard", "0.9144"),
    sq("mile", "1609.344"),
    u("point", Dim::Length, "0.00035277777"),
    u("cable", Dim::Length, "185.3184"),
    u("league", Dim::Length, "4828.032"),
    u("furlong", Dim::Length, "201.168"),
    u("nautical_mile", Dim::Length, "1853.184"),
    u("chain", Dim::Length, "20.1168"),
    u("pixel", Dim::Length, "1"),
    u("em", Dim::Length, "1"),
    u("acre", Dim::Area, "4046.86"),
    u("hectare", Dim::Area, "10000"),
    u("are", Dim::Area, "100"),
    u("ping", Dim::Area, "3.306"),
    si("litre", Dim::Volume, "0.001", Prefixes::Si),
    u("cup", Dim::Volume, "0.000236588237"),
    u("quart", Dim::Volume, "0.000946352946"),
    u("pint", Dim::Volume, "0.000473176473"),
    u("gallon", Dim::Volume, "0.00378541178"),
    u("tablespoon", Dim::Volume, "0.0000147867648"),
    u("teaspoon", Dim::Volume, "0.00000492892159"),
    si("gram", Dim::Mass, "1", Prefixes::Si),
    si("tonne", Dim::Mass, "1000000", Prefixes::Si),
    u("carat", Dim::Mass, "0.2"),
    u("pound", Dim::Mass, "453.59237"),
    u("centner", Dim::Mass, "100000"),
    u("ounce", Dim::Mass, "28.349523125"),
    u("stone", Dim::Mass, "6350.29318"),
    si("second", Dim::Time, "1", Prefixes::Si),
    u("minute", Dim::Time, "60"),
    u("hour", Dim::Time, "3600"),
    u("day", Dim::Time, "86400"),
    u("week", Dim::Time, "604800"),
    u("month", Dim::Time, "2592000"),
    u("year", Dim::Time, "31536000"),
    u("radian", Dim::Angle, "1"),
    u("degree", Dim::Angle, "0.017453292519943295"),
    Builtin {
        id: "celsius",
        dim: Dim::Temperature,
        ratio: "1",
        offset: "273.15",
        prefixes: Prefixes::None,
        powers: false,
    },
    Builtin {
        id: "fahrenheit",
        dim: Dim::Temperature,
        ratio: "5/9",
        offset: "459.67",
        prefixes: Prefixes::None,
        powers: false,
    },
    u("kelvin", Dim::Temperature, "1"),
    si("bit", Dim::Data, "1", Prefixes::Data),
    si("byte", Dim::Data, "8", Prefixes::Data),
];

/// The built-in unit table, in a fixed order.
pub fn builtin_units() -> Vec<UnitDef> {
    BUILTINS
        .iter()
        .map(|b| UnitDef {
            id: b.id.to_string(),
            dim: b.dim,
            factor: match b.id {
                "pixel" => Factor::Pixel,
                "em" => Factor::Em,
                _ => Factor::Ratio(parse_ratio(b.ratio)),
            },
            offset: Num::exact(b.offset),
            prefixes: b.prefixes,
            powers: b.powers,
        })
        .collect()
}

/// `"0.0254"`, or a fraction such as `"5/9"`.
fn parse_ratio(text: &str) -> Num {
    match text.split_once('/') {
        Some((n, d)) => Num::exact(n).div(Num::exact(d)).unwrap_or(Num::ONE),
        None => Num::exact(text),
    }
}

/// A metric or binary prefix: `kilo`/`k` = 1000, `kibi`/`Ki` = 1024.
#[derive(Debug, Clone, Copy)]
pub struct Prefix {
    pub id: &'static str,
    pub factor: &'static str,
    pub binary: bool,
    pub negative: bool,
}

const fn p(id: &'static str, factor: &'static str, negative: bool) -> Prefix {
    Prefix {
        id,
        factor,
        binary: false,
        negative,
    }
}

const fn b(id: &'static str, factor: &'static str) -> Prefix {
    Prefix {
        id,
        factor,
        binary: true,
        negative: false,
    }
}

pub const PREFIXES: &[Prefix] = &[
    p("yotta", "1e24", false),
    p("zetta", "1e21", false),
    p("exa", "1e18", false),
    p("peta", "1e15", false),
    p("tera", "1e12", false),
    p("giga", "1e9", false),
    p("mega", "1e6", false),
    p("kilo", "1e3", false),
    p("hecto", "1e2", false),
    p("deca", "1e1", false),
    p("deci", "1e-1", true),
    p("centi", "1e-2", true),
    p("milli", "1e-3", true),
    p("micro", "1e-6", true),
    p("nano", "1e-9", true),
    p("pico", "1e-12", true),
    p("femto", "1e-15", true),
    p("atto", "1e-18", true),
    p("zepto", "1e-21", true),
    p("yocto", "1e-24", true),
    b("kibi", "1024"),
    b("mebi", "1048576"),
    b("gibi", "1073741824"),
    b("tebi", "1099511627776"),
    b("pebi", "1125899906842624"),
    b("exbi", "1152921504606846976"),
    b("zebi", "1180591620717411303424"),
    b("yobi", "1208925819614629174706176"),
];

/// A concrete unit: a definition, an optional prefix, and a power (2 and 3
/// build square and cubic units from a length, `km²`, `cubic inch`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnitRef {
    pub def: u16,
    pub prefix: Option<u8>,
    pub power: u8,
}

/// Document-level settings that CSS units depend on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitEnv {
    pub ppi: Num,
    pub em_px: Num,
}

impl UnitRef {
    pub fn plain(def: usize) -> Self {
        Self {
            def: u16::try_from(def).unwrap_or(u16::MAX),
            prefix: None,
            power: 1,
        }
    }

    pub fn with_power(self, power: u8) -> Self {
        Self { power, ..self }
    }

    pub fn prefix(self) -> Option<&'static Prefix> {
        self.prefix.and_then(|p| PREFIXES.get(usize::from(p)))
    }
}

/// The unit table and the conversions that need it.
pub trait UnitTable {
    fn unit_def(&self, unit: UnitRef) -> &UnitDef;

    fn dim(&self, unit: UnitRef) -> Dim {
        match unit.power {
            2 => Dim::Area,
            3 => Dim::Volume,
            _ => self.unit_def(unit).dim,
        }
    }

    /// True for length units and their squares and cubes.
    fn is_length_family(&self, unit: UnitRef) -> bool {
        self.unit_def(unit).dim == Dim::Length
    }

    /// Base units per one of `unit`.
    fn ratio(&self, unit: UnitRef, env: UnitEnv) -> Num {
        let mut ratio = match self.unit_def(unit).factor {
            Factor::Ratio(r) => r,
            Factor::Pixel => Num::exact("0.0254").div(env.ppi).unwrap_or(Num::ZERO),
            Factor::Em => Num::exact("0.0254")
                .mul(env.em_px)
                .div(env.ppi)
                .unwrap_or(Num::ZERO),
        };
        if let Some(prefix) = unit.prefix() {
            ratio = ratio.mul(Num::exact(prefix.factor));
        }
        ratio.pow(Num::int(i64::from(unit.power.max(1))))
    }

    /// Convert `value` between two units of the same dimension.
    fn convert(&self, value: Num, from: UnitRef, to: UnitRef, env: UnitEnv) -> Option<Num> {
        if from == to {
            return Some(value);
        }
        let base = value
            .add(self.unit_def(from).offset)
            .mul(self.ratio(from, env));
        Some(base.div(self.ratio(to, env))?.sub(self.unit_def(to).offset))
    }
}

impl UnitTable for [UnitDef] {
    fn unit_def(&self, unit: UnitRef) -> &UnitDef {
        let index = usize::from(unit.def).min(self.len().saturating_sub(1));
        &self[index]
    }
}

pub fn unit_index(units: &[UnitDef], id: &str) -> Option<usize> {
    units.iter().position(|u| u.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: UnitEnv = UnitEnv {
        ppi: Num::Float(96.0),
        em_px: Num::Float(16.0),
    };

    fn unit(units: &[UnitDef], id: &str) -> UnitRef {
        UnitRef::plain(unit_index(units, id).expect("known unit"))
    }

    #[test]
    fn inches_to_centimetres_is_exact() {
        let units = builtin_units();
        let centi = PREFIXES
            .iter()
            .position(|p| p.id == "centi")
            .and_then(|i| u8::try_from(i).ok());
        let cm = UnitRef {
            prefix: centi,
            ..unit(&units, "meter")
        };
        assert_eq!(
            units.convert(Num::int(5), unit(&units, "inch"), cm, ENV),
            Some(Num::exact("12.7"))
        );
    }

    #[test]
    fn temperatures_are_affine() {
        let units = builtin_units();
        let f = units.convert(
            Num::int(100),
            unit(&units, "celsius"),
            unit(&units, "fahrenheit"),
            ENV,
        );
        let shown = f
            .and_then(Num::to_decimal)
            .map(|d| d.round_dp(10).normalize().to_string());
        assert_eq!(shown, Some("212".into()));
    }
}
