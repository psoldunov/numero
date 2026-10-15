//! The General tab: Numi's settings (`docs/numi-spec-ui.md` §1.1) and
//! Numero's own, grouped by what they change rather than in the nib's
//! order. What results look like first, then the theme and text size, the
//! window's place in the desktop, where calculations are kept, and the
//! sizes CSS units convert with.

use gpui::{Context, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use super::Preferences;
use super::controls::{Choice, SECTION_GAP, choices, hint, panel, row, section, switch};
use super::look::Look;
use crate::settings::{Appearance, FontSize, Settings};

impl Preferences {
    pub(super) fn general(
        &self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let look = Look::current(cx);
        let settings = self.settings(cx);
        v_flex()
            .gap(SECTION_GAP)
            .child(self.results_section(&settings, &look, window, cx))
            .child(self.appearance_section(&settings, &look, cx))
            .child(self.window_section(&settings, &look, cx))
            .child(self.folder_section(&look, cx))
            .child(self.units_section(&look, window, cx))
    }

    /// The theme, Numi's Night Mode with System added, and the text size.
    fn appearance_section(
        &self,
        settings: &Settings,
        look: &Look,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let themes = [
            (Appearance::Dark, "Dark"),
            (Appearance::Light, "Light"),
            (Appearance::System, "System"),
        ]
        .into_iter()
        .map(|(appearance, label)| Choice {
            label: label.into(),
            selected: settings.appearance == appearance,
            on_click: Box::new(cx.listener(move |this, _, _, cx| {
                this.update(|settings| settings.appearance = appearance, cx);
            })),
        })
        .collect();
        let sizes = FontSize::ALL
            .into_iter()
            .map(|size| Choice {
                label: size.label().into(),
                selected: settings.text_size == size,
                on_click: Box::new(cx.listener(move |this, _, _, cx| {
                    this.update(|settings| settings.text_size = size, cx);
                })),
            })
            .collect();
        section(
            "Appearance",
            panel(look)
                .child(row("Theme", None, choices("theme", themes, look), look))
                .child(row(
                    "Font size",
                    Some(hint("Dynamic grows with the window", look.muted)),
                    choices("font-size", sizes, look),
                    look,
                )),
            look,
        )
    }

    /// The global shortcut and the window's place in the desktop.
    fn window_section(
        &self,
        settings: &Settings,
        look: &Look,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let tray = if cfg!(target_os = "macos") {
            "Show in the menu bar"
        } else {
            "Show in the system tray"
        };
        let login_error = self
            .login_error
            .clone()
            .map(|error| hint(error, look.danger));
        section(
            "Window",
            panel(look)
                .child(row(
                    "Global shortcut",
                    Some(self.recorder_hint(look)),
                    self.recorder(look, cx),
                    look,
                ))
                .child(row(
                    "Always on top",
                    None,
                    switch(
                        "always-on-top",
                        settings.always_on_top,
                        cx.listener(|this, on: &bool, _, cx| {
                            let on = *on;
                            this.update(|settings| settings.always_on_top = on, cx);
                        }),
                        look,
                    ),
                    look,
                ))
                .child(row(
                    tray,
                    None,
                    switch(
                        "menu-bar-mode",
                        settings.menu_bar_mode,
                        cx.listener(|this, on: &bool, _, cx| {
                            let on = *on;
                            this.update(|settings| settings.menu_bar_mode = on, cx);
                        }),
                        look,
                    ),
                    look,
                ))
                .child(row(
                    "Launch at login",
                    login_error,
                    switch(
                        "launch-at-login",
                        self.launch_at_login,
                        cx.listener(|this, on: &bool, _, cx| {
                            this.set_launch_at_login(*on, cx);
                        }),
                        look,
                    ),
                    look,
                )),
            look,
        )
    }
}
