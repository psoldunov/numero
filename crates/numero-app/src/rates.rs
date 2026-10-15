//! Currency rates for the engine: the cached ones at once, fresh ones from
//! the network in the background when the cache is missing or old.
//!
//! The network is never touched on the UI thread. A failed fetch only logs:
//! the document keeps evaluating with whatever rates it had.

use chrono::{DateTime, Utc};
use gpui::{App, Task};
use numero_engine::Rates;

/// The rates saved last time, if any. Instant.
pub(crate) fn cached() -> Option<Rates> {
    let rates = numero_rates::load_cached();
    match &rates {
        Some(rates) => tracing::info!(
            currencies = rates.per_usd.len(),
            fetched_at = ?rates.fetched_at,
            "loaded cached rates"
        ),
        None => tracing::info!("no cached rates"),
    }
    rates
}

/// Whether `rates` should be fetched again: missing, of unknown age, or
/// older than [`numero_rates::MAX_AGE`].
pub(crate) fn needs_refresh(rates: Option<&Rates>, now: DateTime<Utc>) -> bool {
    let Some(fetched_at) = rates.and_then(|rates| rates.fetched_at) else {
        return true;
    };
    // A time in the future (a clock that moved back) counts as fresh.
    (now - fetched_at)
        .to_std()
        .is_ok_and(|age| age > numero_rates::MAX_AGE)
}

/// Fetches fresh rates on the background executor and saves them to the
/// cache. Resolves to `None` when the fetch fails.
pub(crate) fn fetch_in_background(cx: &App) -> Task<Option<Rates>> {
    cx.background_executor().spawn(async move {
        let rates = match numero_rates::fetch() {
            Ok(rates) => rates,
            Err(error) => {
                tracing::warn!(%error, "couldn't fetch currency rates");
                return None;
            }
        };
        if let Err(error) = numero_rates::save_cache(&rates) {
            tracing::warn!(%error, "couldn't cache currency rates");
        }
        tracing::info!(currencies = rates.per_usd.len(), "fetched currency rates");
        Some(rates)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeDelta;

    use super::*;

    fn fetched(at: Option<DateTime<Utc>>) -> Rates {
        Rates {
            per_usd: HashMap::from([("EUR".to_owned(), 0.9)]),
            fetched_at: at,
        }
    }

    #[test]
    fn missing_or_undated_rates_are_fetched() {
        let now = Utc::now();
        assert!(needs_refresh(None, now));
        assert!(needs_refresh(Some(&fetched(None)), now));
    }

    #[test]
    fn rates_are_fetched_once_older_than_the_maximum_age() {
        let now = Utc::now();
        let max_age = TimeDelta::from_std(numero_rates::MAX_AGE).unwrap();
        let minute = TimeDelta::minutes(1);

        assert!(!needs_refresh(Some(&fetched(Some(now))), now));
        assert!(!needs_refresh(
            Some(&fetched(Some(now - max_age + minute))),
            now
        ));
        assert!(needs_refresh(
            Some(&fetched(Some(now - max_age - minute))),
            now
        ));
        assert!(!needs_refresh(Some(&fetched(Some(now + minute))), now));
    }
}
