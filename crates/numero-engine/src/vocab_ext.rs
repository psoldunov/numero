//! The runtime half of [`Vocab`]: Numi-style unit ids, extension units
//! and functions, and completion hints.

use crate::num::Num;
use crate::units::{Dim, Factor, PREFIXES, Prefixes, UnitDef, UnitRef, UnitTable, unit_index};
use crate::vocab::{Func, Meaning, Vocab, split_variants};

impl Vocab {
    /// Numi-style unit id: `m`, `km`, `m2`, `foot3`, `GiB`, `USD`.
    pub fn unit_id(&self, unit: UnitRef) -> String {
        let def = self.unit_def(unit);
        let base = if def.id == "meter" {
            "m".to_string()
        } else {
            def.id.clone()
        };
        let prefixed = match unit.prefix() {
            Some(prefix) => {
                let symbol = self
                    .prefix_symbols
                    .get(usize::from(unit.prefix.unwrap_or(0)))
                    .cloned()
                    .unwrap_or_default();
                let unit_symbol = self
                    .unit_symbols
                    .get(usize::from(unit.def))
                    .cloned()
                    .unwrap_or_default();
                if symbol.is_empty() {
                    format!("{}{base}", prefix.id)
                } else {
                    format!("{symbol}{unit_symbol}")
                }
            }
            None => base,
        };
        if unit.power > 1 {
            format!("{prefixed}{}", unit.power)
        } else {
            prefixed
        }
    }

    /// The unit or currency behind a Numi-style id (see [`Vocab::unit_id`]).
    pub fn unit_by_id(&self, id: &str) -> Option<UnitLike> {
        if id == "percent" || id == "%" {
            return Some(UnitLike::Percent);
        }
        if let Some(&code) = self.currency_index.get(id) {
            return Some(UnitLike::Currency(code));
        }
        let (stem, power) = match id.strip_suffix(['2', '3']) {
            Some(stem) => (stem, if id.ends_with('2') { 2 } else { 3 }),
            None => (id, 1),
        };
        let stem = if stem == "m" { "meter" } else { stem };
        if let Some(index) = unit_index(&self.units, stem) {
            return Some(UnitLike::Unit(UnitRef {
                power,
                ..UnitRef::plain(index)
            }));
        }
        self.prefixed_by_id(stem)
            .map(|u| UnitLike::Unit(u.with_power(power)))
    }

    fn prefixed_by_id(&self, id: &str) -> Option<UnitRef> {
        (0..self.units.len()).find_map(|index| {
            let format = self.unit_symbols.get(index)?;
            let def = &self.units[index];
            PREFIXES
                .iter()
                .enumerate()
                .filter(|(_, p)| Self::prefix_allowed(def, p))
                .find_map(|(p, prefix)| {
                    let symbol = &self.prefix_symbols[p];
                    let by_symbol = !symbol.is_empty() && id == format!("{symbol}{format}");
                    let by_name = id == format!("{}{}", prefix.id, def.id);
                    (by_symbol || by_name).then(|| UnitRef {
                        prefix: u8::try_from(p).ok(),
                        ..UnitRef::plain(index)
                    })
                })
        })
    }

    /// Register an extension unit: `ratio` base units per one of it. Its
    /// `symbol` (Numi's `internationalSymbol`) is one more phrase.
    pub fn add_unit(
        &mut self,
        id: &str,
        phrases: &str,
        symbol: Option<&str>,
        base: UnitRef,
        ratio: Num,
        format: &str,
    ) -> Option<UnitRef> {
        let dim = self.dim(base);
        let base_ratio = self.ratio(base, crate::doc::DEFAULT_ENV);
        if matches!(self.unit_def(base).factor, Factor::Pixel | Factor::Em)
            || dim == Dim::Temperature
        {
            return None;
        }
        self.units.push(UnitDef {
            id: id.to_string(),
            dim,
            factor: Factor::Ratio(base_ratio.mul(ratio)),
            offset: Num::ZERO,
            prefixes: Prefixes::None,
            powers: false,
        });
        let format = if format.is_empty() {
            id.to_string()
        } else {
            format.to_string()
        };
        self.unit_symbols
            .push(symbol.map_or_else(|| format.clone(), str::to_string));
        self.unit_formats.push(format);
        let unit = UnitRef::plain(self.units.len() - 1);
        for phrase in split_variants(phrases).chain([id]).chain(symbol) {
            self.insert(phrase, Meaning::Unit(unit));
        }
        Some(unit)
    }

    /// Register an extension function's phrases; returns its index.
    pub fn add_function(&mut self, id: &str, phrases: &str) -> u16 {
        let index = u16::try_from(self.functions.len()).unwrap_or(u16::MAX);
        self.functions.push(id.to_string());
        for phrase in split_variants(phrases).chain(std::iter::once(id)) {
            self.insert(phrase, Meaning::Func(Func::Custom(index)));
        }
        index
    }

    /// Known phrases starting with `prefix` (case-insensitive), shortest
    /// first: the engine side of Numi's QuickType hints.
    pub fn hints(&self, prefix: &str, limit: usize) -> Vec<String> {
        let wanted = prefix.to_lowercase();
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<&str> = self
            .trie
            .entries()
            .iter()
            .filter(|e| {
                e.phrase.chars().count() > 2 && e.phrase.to_lowercase().starts_with(&wanted)
            })
            .map(|e| e.phrase.as_str())
            .collect();
        found.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        found.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        found.into_iter().take(limit).map(String::from).collect()
    }
}

/// What a unit id names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnitLike {
    Unit(UnitRef),
    Currency(u16),
    Percent,
}
