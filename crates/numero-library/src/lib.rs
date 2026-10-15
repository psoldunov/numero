//! Numero's calculations on disk: one plain UTF-8 `.txt` file each, in a
//! folder the user picks.
//!
//! The folder may live in iCloud Drive, Syncthing or Dropbox and be shared
//! with a Mac running Numi, so it holds nothing but the calculations: each
//! file is named after its first line (`Trip to Rome.txt`), written
//! atomically through a hidden temporary file, and never renamed on edit.
//! What only this machine cares about, the order of the list and the
//! calculation open last, lives in `<state dir>/library.json`, keyed by the
//! folder.
//!
//! Another device may change the folder at any time. [`Library::rescan`]
//! reports what changed since the library last read or wrote each file, and
//! [`Library::save`] never overwrites a file changed behind its back: it
//! writes a conflict copy beside it instead. [`watch`] tells the app when
//! to rescan.

mod files;
mod id;
mod library;
mod migrate;
mod naming;
mod state;
mod watch;

use std::path::PathBuf;

pub use id::CalculationId;
pub use library::Library;
pub use watch::{FolderWatcher, watch};

/// The calculations folder when the user hasn't picked one:
/// `<data dir>/numero/calculations`.
pub fn default_folder() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("calculations"))
}

/// Where the library keeps what belongs to this machine only:
/// `<data dir>/numero`.
pub fn state_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("numero"))
}

/// A calculation as the list of calculations shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: CalculationId,
    /// Its first non-empty line, if it has one.
    pub title: Option<String>,
    pub text: String,
}

impl Entry {
    /// Whether `query` appears in the calculation, ignoring case. An empty
    /// query matches everything.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.text.to_lowercase().contains(&query)
    }
}

/// The first non-empty line of `text`, trimmed.
pub fn title(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// What [`Library::save`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// The text is in the calculation's file.
    Written,
    /// The file had changed on disk since the library last read it, so it
    /// was left alone and the text went to a new calculation, `copy`, which
    /// is now the open one.
    Conflict { copy: CalculationId },
}

/// What [`Library::rescan`] found changed in the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// New files, now at the top of the list.
    pub added: Vec<CalculationId>,
    /// Files gone from the folder, now gone from the list.
    pub removed: Vec<CalculationId>,
    /// Files whose text differs from what the library last read or wrote.
    /// Each new text is reported once; [`Library::load`] accepts it.
    pub modified: Vec<CalculationId>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}
