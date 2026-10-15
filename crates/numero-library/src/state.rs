//! What the library keeps outside the folder: each folder's order and the
//! calculation open last, in `<state dir>/library.json`.
//!
//! ```json
//! {
//!   "version": 1,
//!   "folders": {
//!     "/home/me/Sync/Calculations": {
//!       "order": ["Trip to Rome.txt", "Rent.txt"],
//!       "last_open": "Rent.txt"
//!     }
//!   }
//! }
//! ```
//!
//! Keeping it out of the folder means two machines syncing the folder never
//! fight over it, and a folder switched away from and back to keeps its
//! order. Names that aren't valid calculation ids are dropped on reading.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::id::CalculationId;

const FILE: &str = "library.json";
const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct StateFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    folders: BTreeMap<String, FolderState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FolderState {
    /// Newest first.
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    last_open: Option<String>,
}

/// One folder's order (newest first) and the calculation open last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folder {
    pub order: Vec<CalculationId>,
    pub last_open: Option<CalculationId>,
}

/// The key `folder` goes by: its absolute path, links resolved when it
/// exists.
pub(crate) fn key(folder: &Path) -> String {
    fs::canonicalize(folder)
        .or_else(|_| std::path::absolute(folder))
        .unwrap_or_else(|_| folder.to_owned())
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn load(state_dir: &Path, key: &str) -> Folder {
    let Some(stored) = read(state_dir).folders.remove(key) else {
        return Folder::default();
    };
    Folder {
        order: stored
            .order
            .into_iter()
            .filter_map(|name| CalculationId::try_from(name).ok())
            .collect(),
        last_open: stored
            .last_open
            .and_then(|name| CalculationId::try_from(name).ok()),
    }
}

pub(crate) fn store(state_dir: &Path, key: &str, folder: &Folder) {
    let stored = FolderState {
        order: folder
            .order
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        last_open: folder.last_open.as_ref().map(|id| id.as_str().to_owned()),
    };
    update(state_dir, |file| {
        file.folders.insert(key.to_owned(), stored);
    });
}

/// Drops what's kept for the folder at `key`.
pub(crate) fn forget(state_dir: &Path, key: &str) {
    update(state_dir, |file| {
        file.folders.remove(key);
    });
}

fn update(state_dir: &Path, change: impl FnOnce(&mut StateFile)) {
    let mut file = read(state_dir);
    change(&mut file);
    file.version = VERSION;
    if let Err(error) = write(state_dir, &file) {
        tracing::error!(%error, path = %path(state_dir).display(), "couldn't save the order of calculations");
    }
}

fn path(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

fn read(state_dir: &Path) -> StateFile {
    let path = path(state_dir);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            tracing::error!(%error, path = %path.display(), "the order of calculations is unreadable; starting over");
            StateFile::default()
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => StateFile::default(),
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "couldn't read the order of calculations");
            StateFile::default()
        }
    }
}

fn write(state_dir: &Path, file: &StateFile) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(file).map_err(io::Error::other)?;
    fs::create_dir_all(state_dir)?;
    let path = path(state_dir);
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json)?;
    fs::rename(&temporary, &path)
}
