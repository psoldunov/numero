//! The extension hooks a scripting layer drives (§12.8, `Sample.js`):
//! `set_global_variable`, `add_unit`, `add_function`.

mod common;

use std::sync::Arc;

use common::{check_with, ctx, engine, shown};
use numero_engine::{ExtensionError, FunctionSpec, ScriptValue, UnitSpec};

fn horse() -> UnitSpec {
    UnitSpec {
        id: "horse".into(),
        phrases: "horse, horses, hrs".into(),
        base_unit_id: "m".into(),
        format: "hrs".into(),
        ratio: 2.4,
        international_symbol: None,
    }
}

#[test]
fn global_variables() {
    let mut engine = engine();
    engine
        .set_global_variable("myvar", ScriptValue::new(5.0, Some("USD")))
        .expect("valid");
    engine
        .set_global_variable("anothervar", 333.0.into())
        .expect("valid");
    engine
        .set_global_variable("len", ScriptValue::new(2.0, Some("km")))
        .expect("valid");
    check_with(
        &engine,
        &[
            ("myvar * 2", "$10"),
            ("anothervar + 1", "334"),
            ("len in m", "2,000 m"),
            ("anothervar = 1\nanothervar", "1"),
        ],
    );
}

#[test]
fn global_variables_validate() {
    let mut engine = engine();
    assert_eq!(
        engine.set_global_variable("x", ScriptValue::new(1.0, Some("parsec"))),
        Err(ExtensionError::UnknownUnit("parsec".into()))
    );
    assert!(engine.set_global_variable("2x", 1.0.into()).is_err());
    assert!(engine.set_global_variable("x", f64::NAN.into()).is_err());
}

#[test]
fn custom_units() {
    let mut engine = engine();
    engine.add_unit(horse()).expect("valid unit");
    check_with(
        &engine,
        &[
            ("2 horses in m", "4.8 m"),
            ("10 m in horses", "4.17 hrs"),
            ("1 horse + 1 m", "3.4 m"),
            ("3 hrs", "3 hrs"),
            ("1 horse in km", "0 km"),
        ],
    );
}

#[test]
fn custom_units_validate() {
    let mut engine = engine();
    let with = |base: &str, ratio: f64| UnitSpec {
        base_unit_id: base.into(),
        ratio,
        ..horse()
    };
    assert_eq!(
        engine.add_unit(with("parsec", 1.0)),
        Err(ExtensionError::UnknownUnit("parsec".into()))
    );
    assert_eq!(
        engine.add_unit(with("USD", 1.0)),
        Err(ExtensionError::UnsupportedBase("USD".into()))
    );
    assert_eq!(
        engine.add_unit(with("celsius", 1.0)),
        Err(ExtensionError::UnsupportedBase("celsius".into()))
    );
    assert!(engine.add_unit(with("m", 0.0)).is_err());
    engine.add_unit(with("km", 2.0)).expect("prefixed base");
    check_with(&engine, &[("1 horse in m", "2,000 m")]);
}

#[test]
fn custom_functions() {
    let mut engine = engine();
    engine.add_function(
        FunctionSpec {
            id: "zum".into(),
            phrases: "zum".into(),
        },
        Arc::new(|values: &[ScriptValue]| {
            Ok(ScriptValue::from(
                values.iter().map(|v| v.number).sum::<f64>(),
            ))
        }),
    );
    engine.add_function(
        FunctionSpec {
            id: "echo".into(),
            phrases: "echo, same".into(),
        },
        Arc::new(|values: &[ScriptValue]| {
            values
                .first()
                .cloned()
                .ok_or_else(|| "no argument".to_string())
        }),
    );
    engine.add_function(
        FunctionSpec {
            id: "boom".into(),
            phrases: "boom".into(),
        },
        Arc::new(|_: &[ScriptValue]| Err("always fails".to_string())),
    );
    check_with(
        &engine,
        &[
            ("zum(2;3)", "5"),
            ("zum(2; 3) * 2", "10"),
            ("echo(5 km)", "5 km"),
            ("same($3)", "$3"),
            ("echo(20%)", "20 %"),
            ("boom(1)", "ERR"),
        ],
    );
}

#[test]
fn extensions_survive_a_locale_change() {
    let mut engine = engine();
    engine.add_unit(horse()).expect("valid unit");
    engine.add_function(
        FunctionSpec {
            id: "zum".into(),
            phrases: "zum".into(),
        },
        Arc::new(|values: &[ScriptValue]| {
            Ok(ScriptValue::from(
                values.iter().map(|v| v.number).sum::<f64>(),
            ))
        }),
    );
    engine.set_locale("fr").expect("French tables");
    check_with(&engine, &[("2 horses in m", "4.8 m"), ("zum(1;1)", "2")]);
}

#[test]
fn dates_cross_as_unix_seconds() {
    let mut engine = engine();
    engine.add_function(
        FunctionSpec {
            id: "later".into(),
            phrases: "later".into(),
        },
        Arc::new(|values: &[ScriptValue]| {
            let date = values.first().ok_or("no date")?;
            Ok(ScriptValue::new(
                date.number + 3600.0,
                date.unit_id.as_deref(),
            ))
        }),
    );
    let results = engine.evaluate_with("later(now)", &ctx());
    assert_eq!(shown(&results[0]), "3:00 PM");
}
