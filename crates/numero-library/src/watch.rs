//! Telling the app when the folder changes, so it can rescan.

use std::fmt;
use std::path::Path;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::id::CalculationId;

/// Watches a folder until dropped.
pub struct FolderWatcher {
    _watcher: RecommendedWatcher,
}

impl fmt::Debug for FolderWatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FolderWatcher")
            .finish_non_exhaustive()
    }
}

/// Calls `on_change`, on a thread of the watcher's, whenever something
/// happens to a calculation's file directly in `folder`: FSEvents on macOS,
/// inotify on Linux. Hidden files, such as the library's own temporary
/// ones, and anything not `.txt` are passed over. Calls come in bursts;
/// debounce them, then call [`crate::Library::rescan`].
pub fn watch(
    folder: &Path,
    on_change: impl Fn() + Send + 'static,
) -> Result<FolderWatcher, String> {
    let handler = move |result: notify::Result<Event>| match result {
        Ok(event) if touches_calculation(&event) => on_change(),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "the calculations folder watcher failed"),
    };
    let mut watcher =
        notify::recommended_watcher(handler).map_err(|error| failed(folder, &error))?;
    watcher
        .watch(folder, RecursiveMode::NonRecursive)
        .map_err(|error| failed(folder, &error))?;
    Ok(FolderWatcher { _watcher: watcher })
}

fn touches_calculation(event: &Event) -> bool {
    event.paths.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(CalculationId::is_valid)
    })
}

fn failed(folder: &Path, error: &notify::Error) -> String {
    let message = format!("couldn't watch {}: {error}", folder.display());
    tracing::warn!("{message}");
    message
}
