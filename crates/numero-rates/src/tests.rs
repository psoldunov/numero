#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use numero_engine::Rates;

use super::*;

const FIAT_OK: &str = include_str!("../tests/fixtures/fiat_ok.json");
const CRYPTO_OK: &str = include_str!("../tests/fixtures/crypto_ok.json");

fn ts(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("numero-rates-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn fiat_parses_and_uppercases_and_drops_bad_rates() {
    let fiat = parse_fiat(FIAT_OK).unwrap();
    assert_eq!(fiat.per_usd["EUR"], 0.86);
    assert_eq!(fiat.per_usd["GBP"], 0.75);
    assert_eq!(fiat.per_usd["USD"], 1.0);
    assert!(!fiat.per_usd.contains_key("BAD"));
    assert!(!fiat.per_usd.contains_key("NEG"));
    assert_eq!(fiat.updated_at, Some(ts(1_760_400_000)));
}

#[test]
fn fiat_error_result_is_parse_error() {
    let body = r#"{"result":"error","error-type":"unsupported-code"}"#;
    assert!(matches!(parse_fiat(body), Err(Error::Parse(_))));
}

#[test]
fn fiat_missing_rates_is_parse_error() {
    let body = r#"{"result":"success","time_last_update_unix":1}"#;
    assert!(matches!(parse_fiat(body), Err(Error::Parse(_))));
}

#[test]
fn fiat_malformed_json_is_parse_error() {
    assert!(matches!(parse_fiat("<html>oops"), Err(Error::Parse(_))));
}

#[test]
fn fiat_without_timestamp_has_no_updated_at() {
    let body = r#"{"result":"success","rates":{"USD":1}}"#;
    assert_eq!(parse_fiat(body).unwrap().updated_at, None);
}

#[test]
fn crypto_inverts_prices_and_skips_missing_and_zero() {
    let crypto = parse_crypto(CRYPTO_OK);
    assert_eq!(crypto["BTC"], 1.0 / 50000.0);
    assert_eq!(crypto["ETH"], 1.0 / 2000.0);
    assert!(!crypto.contains_key("DOGE"), "zero price");
    assert!(!crypto.contains_key("SOL"), "no usd field");
    assert!(!crypto.contains_key("ADA"), "missing coin");
    assert_eq!(crypto.len(), 2);
}

#[test]
fn crypto_malformed_json_is_empty() {
    assert!(parse_crypto("not json").is_empty());
}

#[test]
fn crypto_table_has_unique_ids_and_tickers() {
    let mut ids: Vec<_> = CRYPTO_COINS.iter().map(|(id, _)| *id).collect();
    let mut tickers: Vec<_> = CRYPTO_COINS.iter().map(|(_, t)| *t).collect();
    ids.sort_unstable();
    tickers.sort_unstable();
    ids.dedup();
    tickers.dedup();
    assert_eq!(ids.len(), CRYPTO_COINS.len());
    assert_eq!(tickers.len(), CRYPTO_COINS.len());
}

#[test]
fn merge_crypto_wins_and_usd_is_one() {
    let fiat = HashMap::from([("USD".to_owned(), 1.02), ("BTC".to_owned(), 9.0)]);
    let crypto = HashMap::from([("BTC".to_owned(), 0.00002)]);
    let rates = merge(fiat, crypto, Some(ts(5)));
    assert_eq!(rates.per_usd["BTC"], 0.00002);
    assert_eq!(rates.per_usd["USD"], 1.0);
    assert_eq!(rates.fetched_at, Some(ts(5)));
}

#[test]
fn merge_adds_usd_when_absent() {
    let rates = merge(HashMap::new(), HashMap::new(), None);
    assert_eq!(rates.per_usd["USD"], 1.0);
}

#[test]
fn cache_round_trip() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("nested").join("rates.json");
    let rates = Rates {
        per_usd: HashMap::from([("USD".to_owned(), 1.0), ("EUR".to_owned(), 0.86)]),
        fetched_at: Some(ts(1_760_400_000)),
    };
    save_cache_to(&path, &rates).unwrap();
    assert_eq!(load_cached_from(&path), Some(rates.clone()));

    // Overwrite works and leaves no temp files behind.
    let newer = Rates {
        fetched_at: None,
        ..rates
    };
    save_cache_to(&path, &newer).unwrap();
    assert_eq!(load_cached_from(&path), Some(newer));
    let leftovers = std::fs::read_dir(path.parent().unwrap()).unwrap().count();
    assert_eq!(leftovers, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_file_format() {
    let dir = temp_dir("format");
    let path = dir.join("rates.json");
    let rates = Rates {
        per_usd: HashMap::from([("USD".to_owned(), 1.0)]),
        fetched_at: Some(ts(42)),
    };
    save_cache_to(&path, &rates).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["fetched_at"], 42);
    assert_eq!(value["per_usd"]["USD"], 1.0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_missing_file_is_none() {
    let dir = temp_dir("missing");
    assert_eq!(load_cached_from(&dir.join("rates.json")), None);
}

#[test]
fn load_corrupt_or_wrong_version_is_none() {
    let dir = temp_dir("corrupt");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rates.json");
    std::fs::write(&path, "{not json").unwrap();
    assert_eq!(load_cached_from(&path), None);
    std::fs::write(&path, r#"{"version":99,"fetched_at":null,"per_usd":{}}"#).unwrap();
    assert_eq!(load_cached_from(&path), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn staleness() {
    let now = ts(1_760_400_000);
    let at = |age: Duration| Rates {
        per_usd: HashMap::new(),
        fetched_at: Some(now - age),
    };
    assert!(is_stale(&Rates::default(), now), "no timestamp");
    assert!(!is_stale(&at(Duration::hours(1)), now));
    assert!(!is_stale(&at(Duration::hours(6)), now), "exactly MAX_AGE");
    assert!(is_stale(
        &at(Duration::hours(6) + Duration::seconds(1)),
        now
    ));
    assert!(!is_stale(&at(-Duration::hours(1)), now), "future timestamp");
}
