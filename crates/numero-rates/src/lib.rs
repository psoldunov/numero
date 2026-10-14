//! Currency and crypto rates for Numero: fetched from free public sources,
//! cached on disk so the app starts with the last known rates offline.
//!
//! Contract used by `numero-app` and `numero-cli`:
//! - [`load_cached`] is instant and never touches the network.
//! - [`fetch`] blocks on the network; call it off the UI thread, then
//!   [`save_cache`] the result.

mod cache;
mod parse;

use std::time::Duration;

use chrono::{DateTime, Utc};
use numero_engine::Rates;

pub use cache::{cache_path, load_cached_from, save_cache_to};
pub use parse::{CRYPTO_COINS, Fiat, merge, parse_crypto, parse_fiat};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Network(String),
    #[error("bad response: {0}")]
    Parse(String),
    #[error("cache: {0}")]
    Io(#[from] std::io::Error),
}

/// How old cached rates may get before the app refetches them: Numi's
/// `currencyRefreshRate`, six hours.
pub const MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

const FIAT_URL: &str = "https://open.er-api.com/v6/latest/USD";
const CRYPTO_URL: &str = "https://api.coingecko.com/api/v3/simple/price";
const TIMEOUT: Duration = Duration::from_secs(10);

/// The last rates saved to disk, if any.
pub fn load_cached() -> Option<Rates> {
    load_cached_from(&cache_path()?)
}

/// Save rates to the on-disk cache.
pub fn save_cache(rates: &Rates) -> Result<(), Error> {
    let path = cache_path()
        .ok_or_else(|| Error::Io(std::io::Error::other("no cache directory on this platform")))?;
    save_cache_to(&path, rates)
}

/// Whether `rates` are older than [`MAX_AGE`] at `now`. Rates with no
/// fetch time count as stale; a timestamp in the future counts as fresh.
pub fn is_stale(rates: &Rates, now: DateTime<Utc>) -> bool {
    match rates.fetched_at {
        None => true,
        Some(at) => now
            .signed_duration_since(at)
            .to_std()
            .is_ok_and(|age| age > MAX_AGE),
    }
}

/// Fetch fresh fiat and crypto rates. Blocking.
///
/// Fiat is required; crypto is best-effort and silently skipped on failure.
pub fn fetch() -> Result<Rates, Error> {
    let agent = agent();
    let fiat = parse_fiat(&get(&agent, FIAT_URL)?)?;
    let crypto_url = format!("{CRYPTO_URL}?ids={}&vs_currencies=usd", parse::crypto_ids());
    let crypto = get(&agent, &crypto_url)
        .map(|body| parse_crypto(&body))
        .unwrap_or_default();
    let fetched_at = fiat.updated_at.or_else(|| Some(Utc::now()));
    Ok(merge(fiat.per_usd, crypto, fetched_at))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!("numero/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<String, Error> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Network(e.to_string()))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Network(e.to_string()))
}

#[cfg(test)]
mod tests;
