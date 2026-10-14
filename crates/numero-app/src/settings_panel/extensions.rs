//! The Extensions tab: what JavaScript extensions can do, a button that
//! opens their folder, and what loading them said at start
//! (`docs/numi-spec.md` §12.8).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::button::Button;
use gpui_component::{ActiveTheme as _, Disableable as _, Sizable as _, v_flex};

use super::Preferences;
use super::formatting::heading;
use crate::menus::OpenExtensionsFolder;

const ABOUT: &str = "Extensions are JavaScript files that add your own constants, units and \
functions to Numero. Every .js file in the extensions folder runs once, when Numero starts, \
so restart Numero after changing one. Files written for Numi work as they are.";

/// Each call an extension can make, with what it does.
const CALLS: [(&str, &str); 3] = [
    (
        "numi.setVariable(name, value)",
        "defines a variable every calculation can use, such as a tax rate",
    ),
    (
        "numi.addUnit({ id, phrases, baseUnitId, ratio, format })",
        "adds a unit, converted through the unit it is based on",
    ),
    (
        "numi.addFunction({ id, phrases }, fn)",
        "adds a function, called as name(a; b), whose values come in as objects",
    ),
];

impl Preferences {
    pub(super) fn extensions(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let (dir, log) = self
            .calculator
            .read_with(cx, |calculator, _| {
                (
                    calculator.extensions_dir.clone(),
                    calculator.extension_log.clone(),
                )
            })
            .unwrap_or_default();
        let theme = cx.theme();
        let (muted, mono, panel) = (
            theme.muted_foreground,
            theme.mono_font_family.clone(),
            theme.secondary,
        );
        v_flex()
            .gap_3()
            .child(div().child(ABOUT))
            .child(v_flex().gap_1p5().children(CALLS.map(|(call, what)| {
                v_flex()
                    .child(div().font_family(mono.clone()).text_xs().child(call))
                    .child(div().text_xs().text_color(muted).child(what))
            })))
            .child(
                div().child(
                    Button::new("open-extensions-folder")
                        .small()
                        .outline()
                        .label("Open extensions folder")
                        .disabled(dir.is_none())
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(OpenExtensionsFolder), cx);
                        }),
                ),
            )
            .when_some(dir, |list, dir| {
                list.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(dir.display().to_string()),
                )
            })
            .child(heading("Loaded at start", cx))
            .child(
                div()
                    .id("extension-log")
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .p_2()
                    .rounded(px(6.))
                    .bg(panel)
                    .font_family(mono)
                    .text_xs()
                    .when(log.is_empty(), |panel| {
                        panel.text_color(muted).child("No extensions loaded.")
                    })
                    .children(log.into_iter().map(|line| div().child(line))),
            )
    }
}
