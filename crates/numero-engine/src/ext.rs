//! Extension hooks, the Rust side of Numi's JavaScript API (§12.8,
//! `Sample.js`): `numi.setVariable(name, value)`, `numi.addUnit({…})` and
//! `numi.addFunction({id, phrases}, fn)`. Values cross the boundary the
//! way Numi's do, as a double plus an optional unit id (`m`, `km`, `USD`,
//! `percent`). A scripting layer drives these through
//! [`crate::Engine::set_global_variable`], [`crate::Engine::add_unit`] and
//! [`crate::Engine::add_function`]; this crate embeds no script runtime.

use std::fmt;
use std::sync::Arc;

use crate::eval::Scope;
use crate::num::Num;
use crate::value::{Eval, Moment, Quantity, Unit, Value, fail};
use crate::vocab::{UnitLike, Vocab};

/// Unit id for a date crossing the boundary as Unix seconds.
pub const DATE_UNIT_ID: &str = "date";

/// A value as extensions see it: Numi's `{double, unitId}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptValue {
    pub number: f64,
    /// `None` for a plain number; otherwise a unit id such as `m`, `km2`,
    /// `GiB`, `USD`, `percent`, or `date` (Unix seconds).
    pub unit_id: Option<String>,
}

impl ScriptValue {
    pub fn new(number: f64, unit_id: Option<&str>) -> Self {
        Self {
            number,
            unit_id: unit_id.map(String::from),
        }
    }
}

impl From<f64> for ScriptValue {
    fn from(number: f64) -> Self {
        Self {
            number,
            unit_id: None,
        }
    }
}

/// `numi.addUnit`'s dictionary: one `id` is `ratio` of `base_unit_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitSpec {
    pub id: String,
    /// Comma-separated input phrases (`horse, horses, hrs`).
    pub phrases: String,
    pub base_unit_id: String,
    /// Output format; `%@` marks the number, otherwise it follows it.
    pub format: String,
    pub ratio: f64,
}

/// `numi.addFunction`'s dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSpec {
    pub id: String,
    /// Comma-separated call names (`zum`).
    pub phrases: String,
}

/// An extension function: arguments in, one value or an error message out.
pub type ScriptFunction = Arc<dyn Fn(&[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync>;

#[derive(Clone)]
pub struct ExtFunction {
    pub id: String,
    pub phrases: String,
    pub run: ScriptFunction,
}

impl fmt::Debug for ExtFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtFunction")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionError {
    #[error("unknown unit id {0:?}")]
    UnknownUnit(String),
    #[error("{0:?} cannot be the base of a new unit")]
    UnsupportedBase(String),
    #[error("invalid {0}")]
    Invalid(&'static str),
    #[error("unknown locale {0:?}")]
    UnknownLocale(String),
}

fn number(f: f64) -> Result<Num, ExtensionError> {
    if !f.is_finite() {
        return Err(ExtensionError::Invalid("number"));
    }
    Ok(Num::parse(&f.to_string()).unwrap_or(Num::Float(f)))
}

/// A script value as an engine value; dates are read in `tz`.
pub fn to_value(
    value: &ScriptValue,
    vocab: &Vocab,
    tz: chrono_tz::Tz,
) -> Result<Value, ExtensionError> {
    let n = number(value.number)?;
    let Some(id) = value.unit_id.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(Value::plain(n));
    };
    if id == DATE_UNIT_ID {
        let millis = (value.number * 1000.0).round() as i64;
        let at = chrono::DateTime::from_timestamp_millis(millis)
            .ok_or(ExtensionError::Invalid("date"))?;
        return Ok(Value::Date(Moment {
            at: at.with_timezone(&tz),
            has_time: true,
        }));
    }
    let unit = match vocab
        .unit_by_id(id)
        .ok_or_else(|| ExtensionError::UnknownUnit(id.to_string()))?
    {
        UnitLike::Unit(u) => Unit::Phys(u),
        UnitLike::Currency(c) => Unit::Currency(c),
        UnitLike::Percent => Unit::Percent,
    };
    Ok(Value::Q(Quantity::with(n, unit)))
}

/// An engine value as a script value.
pub fn to_script(value: &Value, vocab: &Vocab) -> ScriptValue {
    match value {
        Value::Date(m) => {
            ScriptValue::new(m.at.timestamp_millis() as f64 / 1000.0, Some(DATE_UNIT_ID))
        }
        Value::Q(q) => {
            let unit_id = match q.unit {
                Unit::None => None,
                Unit::Percent => Some("percent".to_string()),
                Unit::Phys(u) => Some(vocab.unit_id(u)),
                Unit::Currency(c) => vocab.currencies.get(usize::from(c)).map(|c| c.code.clone()),
            };
            ScriptValue {
                number: q.n.to_f64(),
                unit_id,
            }
        }
    }
}

/// Run extension function `index` on evaluated arguments.
pub fn call(index: u16, args: &[Value], scope: &Scope<'_>) -> Eval<Value> {
    let Some(function) = scope.functions.get(usize::from(index)) else {
        return fail("unknown function");
    };
    let script_args: Vec<ScriptValue> = args.iter().map(|v| to_script(v, scope.vocab)).collect();
    match (function.run)(&script_args) {
        Ok(result) => to_value(&result, scope.vocab, scope.ctx.local_tz)
            .map_err(|e| crate::value::EvalError(e.to_string())),
        Err(message) => fail(message),
    }
}
