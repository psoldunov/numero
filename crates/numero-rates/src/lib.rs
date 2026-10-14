//! Currency and crypto rates for Numero: fetched from free public sources,
//! cached on disk so the app starts with the last known rates offline.
//!
//! Contract used by `numero-app` and `numero-cli`:
//! - [`load_cached`] is instant and never touches the network.
//! - [`fetch`] blocks on the network; call it off the UI thread, then
//!   [`save_cache`] the result.

use numero_engine::Rates;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Network(String),
    #[error("bad response: {0}")]
    Parse(String),
    #[error("cache: {0}")]
    Io(#[from] std::io::Error),
}

/// How old cached rates may get before the app refetches them.
pub const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(8 * 60 * 60);

/// The last rates saved to disk, if any.
pub fn load_cached() -> Option<Rates> {
    None
}

/// Fetch fresh fiat and crypto rates. Blocking.
pub fn fetch() -> Result<Rates, Error> {
    Err(Error::Network("not implemented yet".into()))
}

/// Save rates to the on-disk cache.
pub fn save_cache(_rates: &Rates) -> Result<(), Error> {
    Ok(())
}
