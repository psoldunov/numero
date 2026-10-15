//! The calculations folder as other devices change it: a watcher on the
//! folder, a rescan a moment after it fires, the open calculation reloaded
//! when it changed elsewhere and has no unsaved edits, and choosing another
//! folder, or moving the calculations into it, from Preferences.
//!
//! An edit is never lost to sync: when the open calculation changed on
//! disk while it had unsaved edits, the library writes them to a
//! `… (conflict).txt` copy, which becomes the open calculation
//! (`calculations::write_now`).

use std::path::PathBuf;
use std::time::Duration;

use gpui::{Context, Window};

use super::Calculator;
use crate::library::{self, Changes, Library};

/// How long after the watcher fires the folder is read: sync tools write
/// in bursts.
const RESCAN_AFTER: Duration = Duration::from_millis(300);

impl Calculator {
    /// Watches the library's folder, replacing any watch before.
    pub(super) fn watch_library(&mut self, cx: &mut Context<'_, Self>) {
        self._watcher = None;
        self._watch_task = None;
        // GPUI's test scheduler panics when a task is woken from another
        // thread, as the watcher's would be; tests call `rescan` instead.
        if cfg!(test) {
            return;
        }
        let Some(folder) = self
            .library
            .as_ref()
            .map(|library| library.folder().to_owned())
        else {
            return;
        };
        let (sender, receiver) = async_channel::bounded::<()>(1);
        match library::watch(&folder, move || {
            let _ = sender.try_send(());
        }) {
            Ok(watcher) => self._watcher = Some(watcher),
            Err(error) => {
                tracing::warn!(%error, folder = %folder.display(), "can't watch the calculations folder");
                return;
            }
        }
        let window = self.window;
        self._watch_task = Some(cx.spawn(async move |this, cx| {
            while receiver.recv().await.is_ok() {
                cx.background_executor().timer(RESCAN_AFTER).await;
                while receiver.try_recv().is_ok() {}
                let rescanned = cx.update(|cx| {
                    window
                        .update(cx, |_, window, cx| {
                            this.update(cx, |this, cx| this.rescan(window, cx))
                        })
                        .is_ok_and(|updated| updated.is_ok())
                });
                if !rescanned {
                    break;
                }
            }
        }));
    }

    /// Reads the folder again and follows what other devices changed.
    pub(crate) fn rescan(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(library) = self.library.as_mut() else {
            return;
        };
        let changes = library.rescan();
        if changes.is_empty() {
            return;
        }
        tracing::info!(?changes, "the calculations folder changed");
        self.follow_open_calculation(&changes, window, cx);
        self.refresh_entries(cx);
    }

    /// Reloads the open calculation if it changed elsewhere and has no
    /// unsaved edits; keeps it, to be written back, if it was deleted.
    fn follow_open_calculation(
        &mut self,
        changes: &Changes,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(library), Some(current)) = (self.library.as_mut(), self.current.clone()) else {
            return;
        };
        let unsaved = self.revision != self.saved_revision;
        if changes.modified.contains(&current) && !unsaved {
            match library.load(&current) {
                Ok(text) => self.reload(&text, window, cx),
                Err(error) => {
                    tracing::error!(%error, ?current, "couldn't read the changed calculation")
                }
            }
        } else if changes.removed.contains(&current) {
            // Deleted elsewhere while open here: the text on screen is the
            // user's, so it is written back.
            self.revision += 1;
            self.schedule_save(cx);
        }
    }

    /// Shows `text`, changed elsewhere, in place of the open calculation's,
    /// keeping the caret where it was.
    fn reload(&mut self, text: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.text(cx) == text {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            let caret = editor.cursor_position();
            editor.set_value(text.to_owned(), window, cx);
            editor.set_cursor_position(caret, window, cx);
        });
        self.evaluate(cx);
        self.show_notice("Updated from the calculations folder", cx);
    }

    /// The folder the calculations are kept in, if they are kept.
    pub(crate) fn calculations_folder(&self) -> Option<PathBuf> {
        self.library
            .as_ref()
            .map(|library| library.folder().to_owned())
    }

    /// Keeps the calculations in `folder` from now on (the default one for
    /// `None`), and opens the one open last there. Nothing is moved; when
    /// the new folder is empty and the old one isn't, Preferences offers to.
    pub(crate) fn set_calculations_dir(
        &mut self,
        folder: Option<PathBuf>,
        cx: &mut Context<'_, Self>,
    ) -> Result<(), String> {
        let Some(paths) = self.library_paths.clone() else {
            return Err("This system has no data folder to keep calculations in.".into());
        };
        if let Some(folder) = &folder {
            std::fs::create_dir_all(folder)
                .map_err(|error| format!("Couldn't use {}: {error}", folder.display()))?;
        }
        self.save_now(cx);
        let old = self.library.take();
        self.update_settings(|settings| settings.calculations_dir = folder, cx);
        let library = paths.open(self.settings.calculations_dir.as_deref());
        self.move_offer = old
            .filter(|old| !old.is_empty() && library.is_empty() && old.folder() != library.folder())
            .map(|old| old.folder().to_owned());
        self.switch_library(library, cx);
        Ok(())
    }

    /// Moves the calculations from the folder used before into the one in
    /// use now, as Preferences offers when the new one is empty. Returns
    /// how many moved.
    pub(crate) fn move_calculations_here(
        &mut self,
        cx: &mut Context<'_, Self>,
    ) -> Result<usize, String> {
        let (Some(old), Some(paths)) = (self.move_offer.take(), self.library_paths.clone()) else {
            return Ok(0);
        };
        self.save_now(cx);
        let Some(target) = self.calculations_folder() else {
            return Ok(0);
        };
        let source = Library::open(&old, &paths.state_dir);
        let count = source.entries().len();
        self._watcher = None;
        match source.move_into(&target) {
            Ok(library) => {
                self.switch_library(library, cx);
                Ok(count)
            }
            Err(error) => {
                tracing::error!(%error, from = %old.display(), to = %target.display(), "couldn't move the calculations");
                self.switch_library(Library::open(&target, &paths.state_dir), cx);
                Err(format!("Couldn't move every calculation: {error}"))
            }
        }
    }

    /// Uses `library` from now on: opens its last calculation in the
    /// calculator's window and watches its folder.
    fn switch_library(&mut self, library: Library, cx: &mut Context<'_, Self>) {
        self.library = Some(library);
        self.current = None;
        self.watch_library(cx);
        self.in_window(cx, |this, window, cx| this.open_last(window, cx));
        self.refresh_entries(cx);
    }

    /// Opens the calculation open last in the library. An empty library
    /// shows an empty calculation, written once the user types, so a new
    /// synced folder gets no file it didn't ask for.
    pub(super) fn open_last(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(library) = self.library.as_mut() else {
            return;
        };
        if library.is_empty() {
            self.show(None, "", window, cx);
            return;
        }
        match library.open_last("") {
            Ok((id, text)) => self.show(Some(id), &text, window, cx),
            Err(error) => tracing::error!(%error, "couldn't open a calculation"),
        }
    }
}
