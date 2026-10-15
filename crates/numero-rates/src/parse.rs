//! Pure parsing and merging of the two rate sources. No I/O.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use numero_engine::Rates;
use serde::Deserialize;

use crate::Error;

/// CoinGecko id → ticker, the top coins by market cap.
pub const CRYPTO_COINS: &[(&str, &str)] = &[
    ("bitcoin", "BTC"),
    ("ethereum", "ETH"),
    ("tether", "USDT"),
    ("binancecoin", "BNB"),
    ("solana", "SOL"),
    ("ripple", "XRP"),
    ("usd-coin", "USDC"),
    ("cardano", "ADA"),
    ("dogecoin", "DOGE"),
    ("tron", "TRX"),
    ("the-open-network", "TON"),
    ("avalanche-2", "AVAX"),
    ("shiba-inu", "SHIB"),
    ("polkadot", "DOT"),
    ("chainlink", "LINK"),
    ("bitcoin-cash", "BCH"),
    ("litecoin", "LTC"),
    ("near", "NEAR"),
    ("uniswap", "UNI"),
    ("stellar", "XLM"),
    ("monero", "XMR"),
    ("ethereum-classic", "ETC"),
];

/// Fiat rates as parsed from the API, before merging.
#[derive(Debug, Clone, PartialEq)]
pub struct Fiat {
    pub per_usd: HashMap<String, f64>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct FiatResponse {
    result: Option<String>,
    time_last_update_unix: Option<i64>,
    rates: Option<HashMap<String, f64>>,
}

fn usable(rate: f64) -> bool {
    rate.is_finite() && rate > 0.0
}

/// Parse an open.er-api.com `latest/USD` response.
pub fn parse_fiat(body: &str) -> Result<Fiat, Error> {
    let response: FiatResponse =
        serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    if response.result.as_deref() != Some("success") {
        return Err(Error::Parse(format!(
            "result was {:?}, expected \"success\"",
            response.result
        )));
    }
    let rates = response
        .rates
        .ok_or_else(|| Error::Parse("missing rates".into()))?;
    let per_usd: HashMap<String, f64> = rates
        .into_iter()
        .filter(|(_, rate)| usable(*rate))
        .map(|(code, rate)| (code.to_uppercase(), rate))
        .collect();
    if per_usd.is_empty() {
        return Err(Error::Parse("no usable rates".into()));
    }
    let updated_at = response
        .time_last_update_unix
        .and_then(|secs| DateTime::from_timestamp(secs, 0));
    Ok(Fiat {
        per_usd,
        updated_at,
    })
}

/// Parse a CoinGecko simple-price response into ticker → units per USD.
/// Malformed input, missing coins and non-positive prices are skipped.
pub fn parse_crypto(body: &str) -> HashMap<String, f64> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return HashMap::new();
    };
    CRYPTO_COINS
        .iter()
        .filter_map(|(id, ticker)| {
            let price = value.get(*id)?.get("usd")?.as_f64()?;
            usable(price).then(|| ((*ticker).to_owned(), 1.0 / price))
        })
        .collect()
}

/// Fiat first, crypto on top, and `USD` is always exactly 1.
pub fn merge(
    fiat: HashMap<String, f64>,
    crypto: HashMap<String, f64>,
    fetched_at: Option<DateTime<Utc>>,
) -> Rates {
    let mut per_usd = fiat;
    per_usd.extend(crypto);
    per_usd.insert("USD".to_owned(), 1.0);
    Rates {
        per_usd,
        fetched_at,
    }
}

/// The CoinGecko ids joined for the `ids=` query parameter.
pub fn crypto_ids() -> String {
    CRYPTO_COINS
        .iter()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()
        .join(",")
}
