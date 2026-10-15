//! Numi-compatible JavaScript extensions: every `*.js` file under the
//! extensions folder runs once, at startup, in one shared JavaScript
//! context with a `numi` global (`setVariable`, `addUnit`, `addFunction`)
//! and a `log()` function, and drives the engine's extension hooks
//! (`docs/numi-spec.md` §12.8).
//!
//! Contract used by `numero-app` and `numero-cli`:
//! - [`extensions_dir`]: where the files live (the app's "Open extensions
//!   folder" button opens it).
//! - [`load`]: seeds `Default.js` when missing, runs every file into the
//!   engine and returns the log the Extensions tab shows.
//!
//! # Why QuickJS (`rquickjs`)
//!
//! The scripts are small and run once; what matters is startup cost and
//! the per-keystroke call path. QuickJS starts a runtime in well under a
//! millisecond, adds a few hundred KB, builds from C with the dev shell's
//! compiler on Linux and Xcode's on macOS, and has an interrupt handler,
//! which is what lets a runaway script be stopped rather than abandoned.
//! Boa is pure Rust but slower to start and to call into, and limits only
//! loop iterations and recursion, not time. Only the core intrinsics are loaded: no `std`/`os`
//! modules, so scripts get no file system or network.
//!
//! # Threading
//!
//! A QuickJS runtime is not `Send`, and engine functions must be `Send +
//! Sync`, so one dedicated thread owns the runtime ([`host`]). Each
//! extension function the engine calls sends a request to that thread and
//! waits at most [`CALL_TIMEOUT`]; past it the script is interrupted, the
//! call yields no result, and the function fails fast for [`COOLDOWN`] so a
//! document calling it on every line cannot freeze the editor. The thread
//! ends when the last engine holding one of its functions is dropped.

mod convert;
mod files;
mod host;

use std::path::{Path, PathBuf};

use numero_engine::Engine;

pub use files::{DEFAULT_FILE, DEFAULT_SOURCE};
pub use host::{CALL_TIMEOUT, COOLDOWN, FILE_TIMEOUT, TIMED_OUT};

use host::{Event, Host, Script};

/// The folder extensions load from: `<data dir>/numero/extensions`.
pub fn extensions_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("numero").join("extensions"))
}

/// What loading the extensions printed: "Extension path: …", "Loading
/// <file>", script `log()` output and "Error on line N: message".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadLog {
    pub lines: Vec<String>,
}

impl LoadLog {
    /// The lines that report an error.
    pub fn errors(&self) -> impl Iterator<Item = &str> {
        self.lines
            .iter()
            .map(String::as_str)
            .filter(|line| line.starts_with("Error"))
    }
}

/// Run every extension under `dir` into `engine`, creating `dir` and
/// seeding `Default.js` first when they are missing.
pub fn load(dir: &Path, engine: &mut Engine) -> LoadLog {
    let mut log = LoadLog::default();
    log.lines.push(format!("Extension path: {}", dir.display()));
    if let Err(error) = files::seed(dir) {
        log.lines
            .push(format!("Error preparing {}: {error}", dir.display()));
    }
    let (found, errors) = files::scripts(dir);
    log.lines.extend(errors);
    if found.is_empty() {
        return log;
    }
    let scripts = found
        .into_iter()
        .map(|file| Script {
            source: std::fs::read_to_string(&file.path)
                .map_err(|e| format!("could not read {}: {e}", file.name)),
            name: file.name,
        })
        .collect();
    match host::start(scripts) {
        Ok(host) => apply(host, engine, &mut log),
        Err(error) => log.lines.push(format!("Error: {error}")),
    }
    log
}

/// Hand the scripts' requests to the engine, in the order they made them.
fn apply(host: Host, engine: &mut Engine, log: &mut LoadLog) {
    for event in &host.events {
        let outcome = match event {
            Event::Log(line) => {
                log.lines.push(line.clone());
                Ok(())
            }
            Event::SetVariable(name, value) => engine
                .set_global_variable(name, value.clone())
                .map_err(|e| format!("setVariable {name:?}: {e}")),
            Event::AddUnit(spec) => engine
                .add_unit(spec.clone())
                .map_err(|e| format!("addUnit {:?}: {e}", spec.id)),
            Event::AddFunction(spec, index) => {
                engine.add_function(spec.clone(), host.function(*index));
                Ok(())
            }
        };
        if let Err(message) = outcome {
            log.lines.push(format!("Error: {message}"));
        }
    }
}
