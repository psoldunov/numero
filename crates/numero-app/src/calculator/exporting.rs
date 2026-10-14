//! The Export menu: the calculation copied with its results or the results
//! alone, or saved to a file the user picks.

use std::path::PathBuf;

use gpui::{App, ClipboardItem, Context};

use super::Calculator;
use crate::export;

impl Calculator {
    pub(crate) fn copy_with_results(&self, cx: &mut App) {
        let text = export::with_results(&self.text(cx), &self.results);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(crate) fn copy_results_only(&self, cx: &mut App) {
        let text = export::results_only(&self.results);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Asks where to save, then writes the calculation there: as typed for
    /// a `.numi` file, with its results for anything else.
    pub(crate) fn save_as(&self, cx: &mut Context<'_, Self>) {
        let text = self.text(cx);
        let results = self.results.clone();
        let directory = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let name = export::file_name(crate::library::title(&text).as_deref());
        let picked = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |_, cx| {
            let path = match picked.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(error)) => {
                    tracing::error!(%error, "couldn't ask where to save");
                    return;
                }
            };
            let content = export::for_path(&path, &text, &results);
            let written = cx
                .background_executor()
                .spawn(async move { std::fs::write(&path, content).map(|()| path) })
                .await;
            match written {
                Ok(path) => tracing::info!(path = %path.display(), "saved the calculation"),
                Err(error) => tracing::error!(%error, "couldn't save the calculation"),
            }
        })
        .detach();
    }
}
