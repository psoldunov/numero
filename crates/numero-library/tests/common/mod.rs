#![allow(dead_code)]
// Clippy only lets `#[test]` functions unwrap; these helpers serve them.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use numero_library::{CalculationId, Library};
use tempfile::TempDir;

/// A calculations folder and a state directory, both temporary.
pub struct Setup {
    root: TempDir,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    pub fn folder(&self) -> PathBuf {
        self.root.path().join("Calculations")
    }

    pub fn state(&self) -> PathBuf {
        self.root.path().join("state")
    }

    pub fn open(&self) -> Library {
        Library::open(&self.folder(), &self.state())
    }

    /// Writes a file into the folder the way sync or another app would.
    pub fn write_external(&self, name: &str, text: &str) {
        fs::create_dir_all(self.folder()).unwrap();
        fs::write(self.folder().join(name), text).unwrap();
    }

    pub fn read(&self, name: &str) -> String {
        fs::read_to_string(self.folder().join(name)).unwrap()
    }

    /// Every file name in the folder, sorted.
    pub fn listing(&self) -> Vec<String> {
        listing(&self.folder())
    }
}

pub fn listing(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

pub fn id(name: &str) -> CalculationId {
    CalculationId::try_from(name).unwrap()
}

pub fn ids(library: &Library) -> Vec<String> {
    library
        .entries()
        .into_iter()
        .map(|entry| entry.id.to_string())
        .collect()
}
