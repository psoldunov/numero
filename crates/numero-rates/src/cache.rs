//! On-disk cache: a small versioned JSON file, written atomically.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use numero_engine::Rates;
use serde::{Deserialize, Serialize};

use crate::Error;

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    fetched_at: Option<i64>,
    per_usd: HashMap<String, f64>,
}

/// `<cache dir>/numero/rates.json`, if the platform has a cache dir.
pub fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("numero").join("rates.json"))
}

/// Read rates from `path`. `None` on a missing, unreadable or corrupt file.
pub fn load_cached_from(path: &Path) -> Option<Rates> {
    let bytes = std::fs::read(path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != VERSION {
        return None;
    }
    Some(Rates {
        per_usd: file.per_usd,
        fetched_at: file
            .fetched_at
            .and_then(|secs| DateTime::<Utc>::from_timestamp(secs, 0)),
    })
}

/// Write rates to `path`, creating parent dirs. Writes a sibling temp file
/// and renames it over the target so readers never see a partial file.
pub fn save_cache_to(path: &Path, rates: &Rates) -> Result<(), Error> {
    let file = CacheFile {
        version: VERSION,
        fetched_at: rates.fetched_at.map(|t| t.timestamp()),
        per_usd: rates.per_usd.clone(),
    };
    let json = serde_json::to_vec(&file).map_err(|e| Error::Io(e.into()))?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp_name);
    let written = std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(written?)
}
