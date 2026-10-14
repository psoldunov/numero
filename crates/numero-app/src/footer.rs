//! The bottom bar: the gear, which opens the settings, the hint over a
//! result ("Click to copy", "Result copied"), and the arrow, which opens the
//! Export menu.

use gpui::{Anchor, Context, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::h_flex;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::popover::Popover;

use crate::assets::Glyph;
use crate::calculator::Calculator;
use crate::settings_panel;
use crate::widgets::corner_button;

const HEIGHT: f32 = 34.;

impl Calculator {
    pub(crate) fn footer(&self, cx: &Context<'_, Self>) -> impl IntoElement {
        h_flex()
            .h(px(HEIGHT))
            .flex_none()
            .px(px(8.))
            .justify_between()
            .gap_2()
            .child(settings_popover(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_center()
                    .text_xs()
                    .text_color(self.palette.muted)
                    .children(self.footer_hint()),
            )
            .child(div().child(export_menu(cx)))
    }
}

fn settings_popover(cx: &Context<'_, Calculator>) -> impl IntoElement {
    let this = cx.entity();
    Popover::new("settings")
        .anchor(Anchor::BottomLeft)
        .trigger(corner_button("settings", Glyph::Settings, "Settings", cx))
        .content(move |_, _, cx| settings_panel::content(&this, cx))
}

fn export_menu(cx: &Context<'_, Calculator>) -> impl IntoElement {
    let this = cx.entity();
    corner_button("export", Glyph::ArrowRight, "Export", cx).dropdown_menu_with_anchor(
        Anchor::BottomRight,
        move |menu, _, _| {
            let (with, only, save) = (this.clone(), this.clone(), this.clone());
            menu.item(
                PopupMenuItem::new("Copy with results").on_click(move |_, _, cx| {
                    with.update(cx, |calculator, cx| calculator.copy_with_results(cx));
                }),
            )
            .item(
                PopupMenuItem::new("Copy results only").on_click(move |_, _, cx| {
                    only.update(cx, |calculator, cx| calculator.copy_results_only(cx));
                }),
            )
            .separator()
            .item(PopupMenuItem::new("Save as…").on_click(move |_, _, cx| {
                save.update(cx, |calculator, cx| calculator.save_as(cx));
            }))
        },
    )
}
