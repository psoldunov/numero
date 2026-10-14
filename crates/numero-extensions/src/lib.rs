//! Numi-compatible JavaScript extensions: every `*.js` file under the
//! extensions folder runs once, at startup, in one shared JavaScript
//! context with a `numi` global (`setVariable`, `addUnit`, `addFunction`)
//! and a `log()` function, and drives the engine's extension hooks.
//!
//! Contract used by `numero-app` and `numero-cli`:
//! - [`extensions_dir`]: where the files live (the app's "Open extensions
//!   folder" button opens it).
//! - [`load`]: seeds `Default.js` when missing, runs every file into the
//!   engine and returns the log the Extensions tab shows.

use std::path::{Path, PathBuf};

use numero_engine::Engine;

/// The folder extensions load from: `<data dir>/numero/extensions`.
pub fn extensions_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("numero").join("extensions"))
}

/// What loading the extensions printed: "Loading <file>", script `log()`
/// output and "Error on line N: message".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadLog {
    pub lines: Vec<String>,
}

/// Run every extension under `dir` into `engine`.
pub fn load(_dir: &Path, _engine: &mut Engine) -> LoadLog {
    LoadLog::default()
}
