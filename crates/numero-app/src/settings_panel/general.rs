//! The General tab, in the order of Numi's (`docs/numi-spec-ui.md` §1.1):
//! the global shortcut, precision, font size, night mode, then the
//! window's place in the desktop. Numero's own settings follow, apart.

use gpui::prelude::FluentBuilder as _;
use gpui::{Context, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::Input;
use gpui_component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};

use super::Preferences;
use super::formatting::heading;
use crate::settings::{Appearance, FontSize, Settings};
use crate::widgets::{chip, row};

impl Preferences {
    pub(super) fn general(
        &self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let settings = self.settings(cx);
        let recorder = self.recorder(cx);
        let precision = self.precision_control(cx);
        let font_size = self.font_size_chips(&settings, cx);
        let night_mode = self.night_mode_chips(&settings, cx);
        let checkboxes = self.window_checkboxes(&settings, cx);
        let folder = self.folder_section(window, cx);
        let formatting = self.formatting(&settings, cx);
        v_flex()
            .gap_2()
            .child(row("Global shortcut", recorder, cx))
            .child(row("Precision", precision, cx))
            .child(row("Font size", font_size, cx))
            .child(row("Night mode", night_mode, cx))
            .child(checkboxes)
            .child(folder)
            .child(formatting)
    }

    /// `−`, the typed precision, `+`.
    fn precision_control(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        h_flex()
            .gap_1()
            .child(chip(
                "precision-down",
                "−",
                false,
                cx.listener(|this, _, window, cx| this.step_precision(-1, window, cx)),
                cx,
            ))
            .child(Input::new(&self.precision).xsmall().w(px(44.)))
            .child(chip(
                "precision-up",
                "+",
                false,
                cx.listener(|this, _, window, cx| this.step_precision(1, window, cx)),
                cx,
            ))
    }

    fn font_size_chips(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let chips: Vec<_> = FontSize::ALL
            .into_iter()
            .enumerate()
            .map(|(ix, size)| {
                chip(
                    ("font-size", ix),
                    size.label(),
                    settings.text_size == size,
                    cx.listener(move |this, _, _, cx| {
                        this.update(|settings| settings.text_size = size, cx);
                    }),
                    cx,
                )
            })
            .collect();
        h_flex().gap_0p5().children(chips)
    }

    /// On is Numi's night theme, Off its light one; System follows the
    /// desktop, a Numero extra.
    fn night_mode_chips(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let choices = [
            (Appearance::Dark, "On"),
            (Appearance::Light, "Off"),
            (Appearance::System, "System"),
        ];
        let chips: Vec<_> = choices
            .into_iter()
            .enumerate()
            .map(|(ix, (appearance, label))| {
                chip(
                    ("night-mode", ix),
                    label,
                    settings.appearance == appearance,
                    cx.listener(move |this, _, _, cx| {
                        this.update(|settings| settings.appearance = appearance, cx);
                    }),
                    cx,
                )
            })
            .collect();
        h_flex().gap_0p5().children(chips)
    }

    fn window_checkboxes(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let danger = cx.theme().danger;
        v_flex()
            .pt_2()
            .gap_2()
            .child(
                Checkbox::new("always-on-top")
                    .label("Always on top")
                    .checked(settings.always_on_top)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.update(|settings| settings.always_on_top = checked, cx);
                    })),
            )
            .child(
                Checkbox::new("menu-bar-mode")
                    .label(if cfg!(target_os = "macos") {
                        "Show in menu bar"
                    } else {
                        "Show in the system tray"
                    })
                    .checked(settings.menu_bar_mode)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.update(|settings| settings.menu_bar_mode = checked, cx);
                    })),
            )
            .child(
                Checkbox::new("launch-at-login")
                    .label("Launch Numero at login")
                    .checked(self.launch_at_login)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.set_launch_at_login(*checked, cx);
                    })),
            )
            .when_some(self.login_error.clone(), |list, error| {
                list.child(div().pl_6().text_xs().text_color(danger).child(error))
            })
    }

    /// Where the calculations are kept.
    fn folder_section(
        &self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        v_flex()
            .gap_2()
            .child(heading("Calculations folder", cx))
            .child(self.folder(window, cx))
    }
}
