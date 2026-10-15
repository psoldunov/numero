//! The bottom bar: the gear, which opens Preferences; the hint over a
//! result ("Click to copy", "Result copied"); the grand total, which a
//! click copies (`docs/numi-spec-ui.md` §4.3); and the arrow, which opens
//! the Export menu. On Linux, which has no menu bar, that menu also lists
//! the Help menu's commands.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Anchor, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::h_flex;
use gpui_component::menu::{DropdownMenu as _, PopupMenu};

use crate::assets::Glyph;
use crate::calculator::Calculator;
use crate::menus::samples::SAMPLES;
use crate::menus::{
    CopyToClipboard, ExportFile, ImportFile, OpenDocs, OpenExtensionsFolder, OpenPreferences,
    OpenSample,
};
use crate::widgets::corner_button;

const HEIGHT: f32 = 34.;

impl Calculator {
    pub(crate) fn footer(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        h_flex()
            .h(px(HEIGHT))
            .flex_none()
            .px(px(8.))
            .justify_between()
            .gap_2()
            .child(
                corner_button("settings", Glyph::Settings, "Settings", cx).on_click(
                    |_, window, cx| window.dispatch_action(Box::new(OpenPreferences), cx),
                ),
            )
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
            .when_some(self.grand_total.clone(), |footer, total| {
                footer.child(self.total(total.into(), cx))
            })
            .child(div().child(export_menu(cx)))
    }

    /// "Total" and the grand total, on a pill under the pointer.
    fn total(&self, total: SharedString, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let palette = self.palette;
        h_flex()
            .id("grand-total")
            .flex_shrink_0()
            .max_w(px(220.))
            .gap_1p5()
            .px(px(6.))
            .py(px(1.))
            .rounded(px(5.))
            .cursor_pointer()
            .text_sm()
            .text_color(palette.result)
            .hover(move |style| {
                style
                    .bg(palette.result_hover)
                    .text_color(palette.result_hover_text)
            })
            .active(move |style| style.bg(palette.result_pressed))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| this.hover_total(*hovered, cx)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.copy_total(cx)))
            .child(div().text_xs().opacity(0.7).child("Total"))
            .child(div().truncate().child(total))
    }
}

fn export_menu(cx: &Context<'_, Calculator>) -> impl IntoElement + use<> {
    let this = cx.entity();
    corner_button("export", Glyph::ArrowRight, "Export", cx).dropdown_menu_with_anchor(
        Anchor::BottomRight,
        move |menu, window, cx| {
            let only = this.clone();
            let menu = menu
                .menu("Copy to Clipboard", Box::new(CopyToClipboard))
                .item(
                    gpui_component::menu::PopupMenuItem::new("Copy Results Only").on_click(
                        move |_, _, cx| {
                            only.update(cx, |calculator, cx| calculator.copy_results_only(cx));
                        },
                    ),
                )
                .menu("Export…", Box::new(ExportFile))
                .separator()
                .menu("Import…", Box::new(ImportFile));
            if cfg!(target_os = "macos") {
                menu
            } else {
                help_items(menu, window, cx)
            }
        },
    )
}

/// The Help menu's commands, for desktops without a menu bar.
fn help_items(menu: PopupMenu, window: &mut Window, cx: &mut Context<'_, PopupMenu>) -> PopupMenu {
    menu.separator()
        .menu("Syntax Guide", Box::new(OpenDocs))
        .submenu("Samples", window, cx, |menu, _, _| {
            SAMPLES
                .iter()
                .enumerate()
                .fold(menu, |menu, (index, sample)| {
                    menu.menu(sample.name, Box::new(OpenSample(index)))
                })
        })
        .menu("Open Extensions Folder", Box::new(OpenExtensionsFolder))
}
