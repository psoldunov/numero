//! Keeping calculations: the open one saved a moment after each edit and
//! again on quitting, and the list's add, open, search and delete.

use std::time::Duration;

use gpui::{App, Context, Entity, Window};
use gpui_component::input::{InputEvent, InputState};

use super::Calculator;
use crate::library::{CalculationId, Entry};

/// How long after the last edit the calculation is saved.
const SAVE_AFTER: Duration = Duration::from_millis(500);

impl Calculator {
    pub(super) fn schedule_save(&mut self, cx: &mut Context<'_, Self>) {
        let (Some(library), Some(id)) = (self.library.clone(), self.current.clone()) else {
            return;
        };
        // Replacing the task drops, and so cancels, the one before.
        self._save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let Ok((text, revision)) =
                this.read_with(cx, |this, cx| (this.text(cx), this.revision))
            else {
                return;
            };
            let saved = cx
                .background_executor()
                .spawn(async move { library.save(&id, &text) })
                .await;
            match saved {
                Ok(()) => {
                    let _ = this.update(cx, |this, _| this.saved_revision = revision);
                }
                Err(error) => tracing::error!(%error, "couldn't save the calculation"),
            }
        }));
    }

    /// Saves the open calculation at once, if it changed since its last
    /// save: before quitting or switching to another.
    pub(super) fn save_now(&mut self, cx: &App) {
        let (Some(library), Some(id)) = (&self.library, &self.current) else {
            return;
        };
        if self.revision == self.saved_revision {
            return;
        }
        self._save = None;
        match library.save(id, &self.text(cx)) {
            Ok(()) => self.saved_revision = self.revision,
            Err(error) => tracing::error!(%error, "couldn't save the calculation"),
        }
    }

    /// Shows `text` as the open calculation.
    fn show(
        &mut self,
        id: Option<CalculationId>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self._save = None;
        self.current = id;
        self.revision = 0;
        self.saved_revision = 0;
        self.copied = None;
        // `set_value` reports no change, so this evaluates by hand.
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text.to_owned(), window, cx);
            editor.focus(window, cx);
        });
        self.evaluate(cx);
    }

    /// Adds an empty calculation and opens it.
    pub(crate) fn new_calculation(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.save_now(cx);
        let Some(library) = self.library.as_mut() else {
            self.show(None, "", window, cx);
            return;
        };
        match library.create("") {
            Ok(id) => self.show(Some(id), "", window, cx),
            Err(error) => tracing::error!(%error, "couldn't add a calculation"),
        }
    }

    /// Opens the calculation `id`, after saving the one open now.
    pub(crate) fn open_calculation(
        &mut self,
        id: &CalculationId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.current.as_ref() == Some(id) {
            return;
        }
        self.save_now(cx);
        let Some(library) = self.library.as_mut() else {
            return;
        };
        match library.load(id) {
            Ok(text) => {
                library.set_last_open(id);
                self.show(Some(id.clone()), &text, window, cx);
            }
            Err(error) => tracing::error!(%error, ?id, "couldn't open the calculation"),
        }
    }

    /// Asks before deleting `id`: the first press arms the delete, the
    /// second one carries it out.
    pub(crate) fn delete_pressed(
        &mut self,
        id: &CalculationId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.pending_delete.as_ref() == Some(id) {
            self.pending_delete = None;
            self.delete_calculation(id, window, cx);
        } else {
            self.pending_delete = Some(id.clone());
            cx.notify();
        }
    }

    /// Deletes `id`. Deleting the open calculation opens the newest one
    /// left, or a new empty one.
    fn delete_calculation(
        &mut self,
        id: &CalculationId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(library) = self.library.as_mut() else {
            return;
        };
        if let Err(error) = library.delete(id) {
            tracing::error!(%error, ?id, "couldn't delete the calculation");
            return;
        }
        if self.current.as_ref() == Some(id) {
            // Nothing left to save of it.
            self._save = None;
            self.current = None;
            let next = library.entries().into_iter().next();
            match next {
                Some(entry) => {
                    library.set_last_open(&entry.id);
                    self.show(Some(entry.id), &entry.text, window, cx);
                }
                None => self.new_calculation(window, cx),
            }
        }
        self.refresh_entries(cx);
    }

    /// Opens or closes the list of calculations. Opening reads them afresh
    /// and gives the search field the keyboard; closing gives it back to the
    /// editor.
    pub(crate) fn set_list_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.list_open = open;
        self.pending_delete = None;
        if open {
            self.refresh_entries(cx);
            self.search.update(cx, |search, cx| {
                search.set_value("", window, cx);
                search.focus(window, cx);
            });
        } else {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        }
        cx.notify();
    }

    /// Reads the list of calculations, with the open one as it stands in
    /// the editor rather than as last saved.
    pub(crate) fn refresh_entries(&mut self, cx: &mut Context<'_, Self>) {
        let Some(library) = &self.library else {
            self.entries = Vec::new();
            return;
        };
        let live = self.text(cx);
        self.entries = library
            .entries()
            .into_iter()
            .map(|entry| {
                if self.current.as_ref() == Some(&entry.id) {
                    Entry {
                        title: crate::library::title(&live),
                        text: live.clone(),
                        ..entry
                    }
                } else {
                    entry
                }
            })
            .collect();
        cx.notify();
    }

    /// The calculations the search lets through.
    pub(crate) fn visible_entries(&self, cx: &App) -> Vec<Entry> {
        let query = self.search.read(cx).value();
        self.entries
            .iter()
            .filter(|entry| entry.matches(&query))
            .cloned()
            .collect()
    }

    pub(super) fn on_search_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.pending_delete = None;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {
                if let Some(first) = self.visible_entries(cx).into_iter().next() {
                    self.open_calculation(&first.id, window, cx);
                    self.set_list_open(false, window, cx);
                }
            }
            InputEvent::Focus | InputEvent::Blur => {}
        }
    }
}
