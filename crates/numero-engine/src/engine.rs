//! [`Engine`]: settings, rates, locale and extensions, and the entry
//! points that evaluate a document.

use std::fmt;
use std::sync::Arc;

use crate::doc;
use crate::ext::{
    self, ExtFunction, ExtensionError, FunctionSpec, ScriptFunction, ScriptValue, UnitSpec,
};
use crate::num::Num;
use crate::value::Value;
use crate::vocab::{self, UnitLike, Vocab};
use crate::{Config, EvalContext, LineResult, Rates};

/// The engine: phrase and unit tables, rates, settings and extensions.
/// Cheap to clone; evaluation does not mutate it.
#[derive(Clone)]
pub struct Engine {
    config: Config,
    rates: Rates,
    locale: String,
    vocab: Arc<Vocab>,
    globals: Vec<(String, ScriptValue)>,
    units: Vec<UnitSpec>,
    functions: Vec<ExtFunction>,
}

/// A whole document's results plus Numi's bottom-bar grand total: every
/// line result added together, skipping the ones that cannot be added.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub lines: Vec<LineResult>,
    pub grand_total: Option<String>,
    pub grand_total_number: Option<f64>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("config", &self.config)
            .field("rates", &self.rates.per_usd.len())
            .field("locale", &self.locale)
            .field("globals", &self.globals)
            .field("units", &self.units)
            .field("functions", &self.functions)
            .finish()
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            rates: Rates::default(),
            locale: "en".into(),
            vocab: vocab::english(),
            globals: Vec::new(),
            units: Vec::new(),
            functions: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.config = config;
    }

    pub fn set_rates(&mut self, rates: Rates) {
        self.rates = rates;
    }

    pub fn rates(&self) -> &Rates {
        &self.rates
    }

    /// Locales with phrase tables (`en`, `de`, `pt-BR`…).
    pub fn locales() -> Vec<&'static str> {
        vocab::locales().collect()
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Accept phrases in `locale` as well as English, and format units the
    /// locale's way. Extension units and functions carry over.
    pub fn set_locale(&mut self, locale: &str) -> Result<(), ExtensionError> {
        let base = vocab::for_locale(locale)
            .ok_or_else(|| ExtensionError::UnknownLocale(locale.into()))?;
        self.locale = locale.to_string();
        self.vocab = base;
        let units = std::mem::take(&mut self.units);
        let functions = std::mem::take(&mut self.functions);
        for unit in units {
            self.add_unit(unit)?;
        }
        for function in functions {
            Arc::make_mut(&mut self.vocab).add_function(&function.id, &function.phrases);
            self.functions.push(function);
        }
        Ok(())
    }

    /// `numi.setVariable`: a variable every document starts with. A
    /// document can still reassign it.
    pub fn set_global_variable(
        &mut self,
        name: &str,
        value: ScriptValue,
    ) -> Result<(), ExtensionError> {
        let valid_name = name.chars().next().is_some_and(char::is_alphabetic)
            && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !valid_name {
            return Err(ExtensionError::Invalid("variable name"));
        }
        ext::to_value(&value, &self.vocab, chrono_tz::UTC)?;
        self.globals.retain(|(existing, _)| existing != name);
        self.globals.push((name.to_string(), value));
        Ok(())
    }

    /// `numi.addUnit`: a unit defined as `ratio` of an existing one
    /// (`horse` = 2.4 `m`). Temperatures, CSS units and currencies cannot
    /// be bases.
    pub fn add_unit(&mut self, spec: UnitSpec) -> Result<(), ExtensionError> {
        if spec.id.trim().is_empty() {
            return Err(ExtensionError::Invalid("unit id"));
        }
        if !spec.ratio.is_finite() || spec.ratio <= 0.0 {
            return Err(ExtensionError::Invalid("unit ratio"));
        }
        let base = match self.vocab.unit_by_id(&spec.base_unit_id) {
            Some(UnitLike::Unit(unit)) => unit,
            Some(_) => return Err(ExtensionError::UnsupportedBase(spec.base_unit_id.clone())),
            None => return Err(ExtensionError::UnknownUnit(spec.base_unit_id.clone())),
        };
        let ratio = Num::parse(&spec.ratio.to_string()).unwrap_or(Num::Float(spec.ratio));
        let symbol = spec
            .international_symbol
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        Arc::make_mut(&mut self.vocab)
            .add_unit(&spec.id, &spec.phrases, symbol, base, ratio, &spec.format)
            .ok_or_else(|| ExtensionError::UnsupportedBase(spec.base_unit_id.clone()))?;
        self.units.push(spec);
        Ok(())
    }

    /// `numi.addFunction`: callable as `id(2; 3)` or by any phrase.
    pub fn add_function(&mut self, spec: FunctionSpec, function: ScriptFunction) {
        let vocab = Arc::make_mut(&mut self.vocab);
        vocab.add_function(&spec.id, &spec.phrases);
        self.functions.push(ExtFunction {
            id: spec.id,
            phrases: spec.phrases,
            run: function,
        });
    }

    /// Phrases that start with `prefix`, shortest first: completion hints
    /// in the spirit of Numi's QuickType engine API.
    pub fn hints(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.vocab.hints(prefix, limit)
    }

    /// Evaluate a whole document against the system clock and local zone.
    /// Returns exactly one result per `\n`-separated line.
    pub fn evaluate(&self, document: &str) -> Vec<LineResult> {
        self.evaluate_with(document, &EvalContext::system())
    }

    /// Evaluate a whole document against an explicit clock and zone.
    pub fn evaluate_with(&self, document: &str, ctx: &EvalContext) -> Vec<LineResult> {
        self.run(document, ctx).lines
    }

    /// Evaluate a document and also compute its grand total.
    pub fn evaluate_document(&self, document: &str, ctx: &EvalContext) -> Evaluation {
        let outcome = self.run(document, ctx);
        let total = outcome.grand_total;
        Evaluation {
            lines: outcome.lines,
            grand_total: total.map(|v| v.display(&self.vocab, &self.config, ctx)),
            grand_total_number: total.and_then(|v| v.number()),
        }
    }

    fn run(&self, document: &str, ctx: &EvalContext) -> doc::Outcome {
        let globals: Vec<(String, Value)> = self
            .globals
            .iter()
            .filter_map(|(name, value)| {
                Some((
                    name.clone(),
                    ext::to_value(value, &self.vocab, ctx.local_tz).ok()?,
                ))
            })
            .collect();
        let inputs = doc::Inputs {
            config: &self.config,
            rates: &self.rates,
            vocab: &self.vocab,
            ctx,
            globals,
            functions: &self.functions,
        };
        doc::evaluate(document, &inputs)
    }

    /// Build the English phrase tables now rather than on the first
    /// evaluation, so the first keystroke is not the slow one.
    pub fn warm_up() {
        let _ = vocab::english();
    }
}
