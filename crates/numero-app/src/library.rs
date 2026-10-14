//! The calculations on disk, through `numero_library`: one `.txt` file
//! each, in a folder the user may point at iCloud Drive, Syncthing or
//! Dropbox, with the list's order kept apart, on this machine only.
//!
//! This module adds what the app needs around it: the welcome sample and
//! where the library lives when the user hasn't chosen a folder.

use std::path::{Path, PathBuf};

pub(crate) use numero_library::{
    CalculationId, Changes, Entry, FolderWatcher, Library, Saved, title, watch,
};

/// What the first run shows: Numi's own welcome sample.
pub(crate) const SAMPLE: &str = "# Sample\n8 / (45 - 20%)\n5 inches in cm\n$9 in Euro";

/// Where libraries live: the machine-only state, and the folder used when
/// the user hasn't picked one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LibraryPaths {
    pub state_dir: PathBuf,
    pub default_folder: PathBuf,
}

impl LibraryPaths {
    /// Under the platform's data directory, or `None` when it has none.
    pub(crate) fn from_env() -> Option<Self> {
        Some(Self {
            state_dir: numero_library::state_dir()?,
            default_folder: numero_library::default_folder()?,
        })
    }

    /// Everything under `dir`, as tests keep it.
    #[cfg(test)]
    pub(crate) fn in_dir(dir: &Path) -> Self {
        Self {
            state_dir: dir.to_owned(),
            default_folder: dir.join("calculations"),
        }
    }

    /// The folder to use: the one `chosen`, else the default.
    pub(crate) fn folder(&self, chosen: Option<&Path>) -> PathBuf {
        chosen.map_or_else(|| self.default_folder.clone(), Path::to_owned)
    }

    /// The library in the folder to use.
    pub(crate) fn open(&self, chosen: Option<&Path>) -> Library {
        Library::open(&self.folder(chosen), &self.state_dir)
    }
}
