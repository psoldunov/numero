//! Numi's JavaScript value dictionaries and the engine's types.
//!
//! A value dict is `{double | decimal, unit?, unitId?, type?}`; a bare
//! number is accepted wherever a dict is. Dates cross as Unix seconds with
//! unit id `date`.

use numero_engine::{FunctionSpec, ScriptValue, UnitSpec};
use rquickjs::{Ctx, Object, Value};

const DATE: &str = "date";

/// A JavaScript value (number or value dict) as a [`ScriptValue`].
pub fn script_value(value: &Value<'_>) -> Result<ScriptValue, String> {
    if let Some(number) = value.as_number() {
        return Ok(number.into());
    }
    let Some(dict) = value.as_object() else {
        return Err("expected a number or a value dictionary".into());
    };
    let number = number_field(dict, "double")?
        .or(number_field(dict, "decimal")?)
        .ok_or("value dictionary has no \"double\" or \"decimal\"")?;
    let unit_id = match string_field(dict, "unitId")?.or(string_field(dict, "unit")?) {
        Some(id) => Some(id),
        None if string_field(dict, "type")?.as_deref() == Some(DATE) => Some(DATE.into()),
        None => None,
    };
    Ok(ScriptValue {
        number,
        unit_id: unit_id.filter(|id| !id.is_empty()),
    })
}

/// A [`ScriptValue`] as the dict an extension function receives.
pub fn value_dict<'js>(ctx: &Ctx<'js>, value: &ScriptValue) -> rquickjs::Result<Object<'js>> {
    let dict = Object::new(ctx.clone())?;
    dict.set("double", value.number)?;
    dict.set("decimal", value.number)?;
    let kind = match value.unit_id.as_deref() {
        Some(DATE) => DATE,
        Some(_) => "unit",
        None => "number",
    };
    dict.set("type", kind)?;
    if let Some(id) = &value.unit_id {
        dict.set("unitId", id.as_str())?;
        dict.set("unit", id.as_str())?;
    }
    Ok(dict)
}

/// `numi.addUnit`'s dictionary.
pub fn unit_spec(dict: &Object<'_>) -> Result<UnitSpec, String> {
    let id = required_string(dict, "id")?;
    Ok(UnitSpec {
        phrases: phrases(dict)?.unwrap_or_else(|| id.clone()),
        base_unit_id: required_string(dict, "baseUnitId")?,
        format: string_field(dict, "format")?.unwrap_or_else(|| id.clone()),
        ratio: number_field(dict, "ratio")?.ok_or("missing \"ratio\"")?,
        international_symbol: string_field(dict, "internationalSymbol")?,
        id,
    })
}

/// `numi.addFunction`'s dictionary.
pub fn function_spec(dict: &Object<'_>) -> Result<FunctionSpec, String> {
    let id = required_string(dict, "id")?;
    Ok(FunctionSpec {
        phrases: phrases(dict)?.unwrap_or_else(|| id.clone()),
        id,
    })
}

/// `phrases` as Numi's comma-separated string; an array of strings is
/// joined into one.
fn phrases(dict: &Object<'_>) -> Result<Option<String>, String> {
    let value: Value<'_> = dict.get("phrases").map_err(|e| e.to_string())?;
    if let Some(array) = value.as_array() {
        let parts = array
            .iter::<String>()
            .collect::<rquickjs::Result<Vec<_>>>()
            .map_err(|_| "\"phrases\" must be a string")?;
        return Ok(Some(parts.join(", ")));
    }
    string_field(dict, "phrases")
}

fn required_string(dict: &Object<'_>, key: &str) -> Result<String, String> {
    string_field(dict, key)?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing \"{key}\""))
}

fn string_field(dict: &Object<'_>, key: &str) -> Result<Option<String>, String> {
    let value: Value<'_> = dict.get(key).map_err(|e| e.to_string())?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    match value.as_string() {
        Some(s) => s.to_string().map(Some).map_err(|e| e.to_string()),
        None => Err(format!("\"{key}\" must be a string")),
    }
}

/// A number field; numeric strings count too, since Numi's `decimal`
/// may be one.
fn number_field(dict: &Object<'_>, key: &str) -> Result<Option<f64>, String> {
    let value: Value<'_> = dict.get(key).map_err(|e| e.to_string())?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    if let Some(number) = value.as_number() {
        return Ok(Some(number));
    }
    value
        .as_string()
        .and_then(|s| s.to_string().ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(Some)
        .ok_or_else(|| format!("\"{key}\" must be a number"))
}
