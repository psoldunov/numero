//! The Extensions tab: what JavaScript extensions can do, a button that
//! opens their folder, and what loading them said at start
//! (`docs/numi-spec.md` §12.8).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};

use super::Preferences;
use super::controls::{SECTION_GAP, button, hint, panel, path_field, section};
use super::look::Look;
use crate::menus::OpenExtensionsFolder;

const ABOUT: &str = "Extensions are JavaScript files that add your own constants, units and \
functions to Numero. Every .js file in the extensions folder runs once, when Numero starts, \
so restart Numero after changing one. Files written for Numi work as they are.";

/// Each call an extension can make, with what it does.
const CALLS: [(&str, &str); 3] = [
    (
        "numi.setVariable(name, value)",
        "Defines a variable every calculation can use, such as a tax rate.",
    ),
    (
        "numi.addUnit({ id, phrases, baseUnitId, ratio, format })",
        "Adds a unit, converted through the unit it is based on.",
    ),
    (
        "numi.addFunction({ id, phrases }, fn)",
        "Adds a function, called as name(a; b), whose values come in as objects.",
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
        let look = Look::current(cx);
        let mono = cx.theme().mono_font_family.clone();
        let folder = v_flex()
            .gap(px(10.))
            .child(div().text_color(look.ink).child(ABOUT))
            .when_some(dir.clone(), |folder, dir| {
                folder.child(path_field(dir.display().to_string(), &look))
            })
            .child(
                h_flex().child(
                    button(
                        "open-extensions-folder",
                        "Open extensions folder",
                        &look,
                        cx,
                    )
                    .disabled(dir.is_none())
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(OpenExtensionsFolder), cx);
                    }),
                ),
            );
        let calls = CALLS.map(|(call, what)| {
            v_flex()
                .gap(px(2.))
                .child(
                    div()
                        .font_family(mono.clone())
                        .text_xs()
                        .text_color(look.ink)
                        .child(call),
                )
                .child(hint(what, look.muted))
        });
        let log = div()
            .id("extension-log")
            .max_h(px(160.))
            .overflow_y_scroll()
            .font_family(mono)
            .text_xs()
            .text_color(look.ink)
            .when(log.is_empty(), |list| {
                list.text_color(look.muted).child("No extensions loaded.")
            })
            .children(log.into_iter().map(|line| div().child(line)));
        v_flex()
            .gap(SECTION_GAP)
            .child(panel(&look).py(px(12.)).child(folder))
            .child(section(
                "What an extension can call",
                panel(&look)
                    .py(px(12.))
                    .child(v_flex().gap(px(12.)).children(calls)),
                &look,
            ))
            .child(section(
                "Loaded at start",
                panel(&look).py(px(12.)).child(log),
                &look,
            ))
    }
}
