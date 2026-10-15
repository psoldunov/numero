//! The calculations folder: where the `.txt` files live, which may be a
//! folder iCloud Drive, Syncthing or Dropbox syncs. "Choose…" picks
//! another, "Reset to default" goes back to the one under the data
//! directory. Nothing is moved unless the user asks: when the new folder
//! is empty and the old one isn't, a button offers to move them there.

use gpui::prelude::FluentBuilder as _;
use gpui::{Context, IntoElement, ParentElement as _, PathPromptOptions, Styled as _, px};
use gpui_component::{Disableable as _, h_flex, v_flex};

use super::Preferences;
use super::controls::{button, hint, panel, path_field, primary_button, section};
use super::look::Look;

const ABOUT: &str = "Each calculation is a .txt file in this folder. Pick one that Syncthing, \
Dropbox or iCloud Drive syncs to use them on more than one computer.";

impl Preferences {
    /// Where the calculations are kept, and the buttons that change it.
    pub(super) fn folder_section(
        &self,
        look: &Look,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let (folder, custom, offer) = self
            .calculator
            .read_with(cx, |calculator, _| {
                (
                    calculator.calculations_folder(),
                    calculator.settings.calculations_dir.is_some(),
                    calculator.move_offer.clone(),
                )
            })
            .unwrap_or_default();
        let shown = folder.map_or_else(
            || "Not kept: this system has no data folder".to_owned(),
            |folder| folder.display().to_string(),
        );
        let buttons = h_flex()
            .gap_2()
            .child(
                button("choose-folder", "Choose…", look, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))),
            )
            .child(
                button("default-folder", "Reset to default", look, cx)
                    .disabled(!custom)
                    .on_click(cx.listener(|this, _, _, cx| this.set_folder(None, cx))),
            );
        let offer = offer.map(|old| {
            v_flex()
                .gap_2()
                .child(hint(
                    format!(
                        "This folder has no calculations yet. Yours are still in {}.",
                        old.display()
                    ),
                    look.ink,
                ))
                .child(
                    h_flex().child(
                        primary_button("move-calculations", "Move them here", look, cx)
                            .on_click(cx.listener(|this, _, _, cx| this.move_calculations(cx))),
                    ),
                )
        });
        let status = self
            .folder_status
            .clone()
            .map(|status| hint(status, look.ink));
        section(
            "Calculations folder",
            panel(look).py(px(12.)).child(
                v_flex()
                    .gap(px(10.))
                    .child(hint(ABOUT, look.muted))
                    .child(path_field(shown, look))
                    .child(buttons)
                    .children(offer)
                    .when_some(status, |panel, status| panel.child(status)),
            ),
            look,
        )
    }

    /// Asks for a folder and keeps the calculations there from now on.
    fn choose_folder(&mut self, cx: &mut Context<'_, Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Use this folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let folder = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    tracing::error!(%error, "couldn't ask for a folder");
                    None
                }
            };
            if let Some(folder) = folder {
                let _ = this.update(cx, |this, cx| this.set_folder(Some(folder), cx));
            }
        })
        .detach();
    }

    fn set_folder(&mut self, folder: Option<std::path::PathBuf>, cx: &mut Context<'_, Self>) {
        self.folder_status = None;
        let changed = self.calculator.update(cx, |calculator, cx| {
            calculator.set_calculations_dir(folder, cx)
        });
        if let Ok(Err(error)) = changed {
            self.folder_status = Some(error);
        }
        cx.notify();
    }

    fn move_calculations(&mut self, cx: &mut Context<'_, Self>) {
        let moved = self
            .calculator
            .update(cx, |calculator, cx| calculator.move_calculations_here(cx));
        self.folder_status = match moved {
            Ok(Ok(count)) => Some(format!("Moved {count} calculations.")),
            Ok(Err(error)) => Some(error),
            Err(_) => None,
        };
        cx.notify();
    }
}
