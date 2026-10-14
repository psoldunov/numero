//! The Export menu: the calculation copied with its results or the results
//! alone, or saved to a file the user picks; File → Import, which reads a
//! text file into a new calculation; and the extensions folder.

use std::path::{Path, PathBuf};

use gpui::{App, ClipboardItem, Context, PathPromptOptions};

use super::Calculator;
use super::main_window::with_calculator;
use crate::export;

/// The largest file Import reads: a calculation is a page of text.
const IMPORT_LIMIT: u64 = 1024 * 1024;

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

    /// Asks for a text file and opens it as a new calculation. Numi reads
    /// `.txt` and `.numi`; any UTF-8 text file is taken.
    pub(crate) fn import_file(&mut self, cx: &mut Context<'_, Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => path,
                    None => return,
                },
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(error)) => {
                    tracing::error!(%error, "couldn't ask which file to import");
                    return;
                }
            };
            let read = cx
                .background_executor()
                .spawn(async move { read_text_file(&path) })
                .await;
            cx.update(|cx| match read {
                Ok(text) => with_calculator(cx, move |calculator, window, cx| {
                    calculator.open_new_with(&text, window, cx);
                }),
                Err(error) => {
                    tracing::error!(%error, "couldn't import the file");
                    let _ = this.update(cx, |this, cx| this.show_notice(error, cx));
                }
            });
        })
        .detach();
    }

    /// Opens the extensions folder in the file manager, making it first.
    pub(crate) fn open_extensions_folder(&mut self, cx: &mut Context<'_, Self>) {
        let Some(dir) = self.extensions_dir.clone() else {
            self.show_notice("This system has no folder for extensions", cx);
            return;
        };
        if let Err(error) = std::fs::create_dir_all(&dir) {
            tracing::error!(%error, dir = %dir.display(), "couldn't make the extensions folder");
            self.show_notice("Couldn't make the extensions folder", cx);
            return;
        }
        cx.open_with_system(&dir);
    }
}

/// The text of the file at `path`, refused when it is large or isn't text.
pub(crate) fn read_text_file(path: &Path) -> Result<String, String> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let size = std::fs::metadata(path)
        .map_err(|error| format!("Couldn't read {name}: {error}"))?
        .len();
    if size > IMPORT_LIMIT {
        return Err(format!("{name} is too large to import"));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("Couldn't read {name}: {error}"))?;
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => Ok(text.replace("\r\n", "\n")),
        _ => Err(format!("{name} isn't a text file")),
    }
}
