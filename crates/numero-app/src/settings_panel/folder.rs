//! The calculations folder: where the `.txt` files live, which may be a
//! folder iCloud Drive, Syncthing or Dropbox syncs. "Choose…" picks
//! another, "Reset to default" goes back to the one under the data
//! directory. Nothing is moved unless the user asks: when the new folder
//! is empty and the old one isn't, a button offers to move them there.

use gpui::prelude::FluentBuilder as _;
use gpui::{Context, IntoElement, ParentElement as _, PathPromptOptions, Styled as _, Window, div};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};

use super::Preferences;

impl Preferences {
    pub(super) fn folder(
        &self,
        _: &mut Window,
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
        let muted = cx.theme().muted_foreground;
        let shown = folder.map_or_else(
            || "Not kept: this system has no data folder".to_owned(),
            |folder| folder.display().to_string(),
        );
        v_flex()
            .gap_2()
            .child(div().text_xs().text_color(muted).child(shown))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("choose-folder")
                            .small()
                            .outline()
                            .label("Choose…")
                            .on_click(cx.listener(|this, _, _, cx| this.choose_folder(cx))),
                    )
                    .child(
                        Button::new("default-folder")
                            .small()
                            .ghost()
                            .label("Reset to default")
                            .disabled(!custom)
                            .on_click(cx.listener(|this, _, _, cx| this.set_folder(None, cx))),
                    ),
            )
            .when_some(offer, |section, old| {
                section.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_xs().text_color(muted).child(format!(
                            "This folder has no calculations yet. Yours are still in {}.",
                            old.display()
                        )))
                        .child(
                            div().child(
                                Button::new("move-calculations")
                                    .small()
                                    .label("Move my calculations there")
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.move_calculations(cx)),
                                    ),
                            ),
                        ),
                )
            })
            .when_some(self.folder_status.clone(), |section, status| {
                section.child(div().text_xs().text_color(muted).child(status))
            })
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
