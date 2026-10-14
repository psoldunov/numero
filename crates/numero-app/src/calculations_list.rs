//! All calculations: a popover under the title bar's list button, with a
//! search field on top and the calculations below, newest first, each by
//! its first line. Typing filters them; a click or Enter opens one. The
//! trash on a row asks once, then deletes.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Anchor, App, Context, Div, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::input::Input;
use gpui_component::popover::Popover;
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};

use crate::assets::Glyph;
use crate::calculator::Calculator;
use crate::library::Entry;
use crate::widgets::corner_button;

/// Numi's list panel is 230 px wide (`docs/numi-spec-ui.md` §5.1).
const WIDTH: f32 = 230.;
/// The open calculation's mark.
const MARK_SIZE: f32 = 6.;
const LIST_HEIGHT: f32 = 260.;

/// The list button and its popover.
pub(crate) fn popover(calculator: &Calculator, cx: &Context<'_, Calculator>) -> impl IntoElement {
    let this = cx.entity();
    let toggled = this.clone();
    Popover::new("calculations")
        .anchor(Anchor::TopRight)
        .open(calculator.list_open)
        .on_open_change(move |open, window, cx| {
            toggled.update(cx, |calculator, cx| {
                calculator.set_list_open(*open, window, cx)
            });
        })
        .trigger(corner_button(
            "calculations",
            Glyph::Menu,
            "All calculations",
            cx,
        ))
        .content(move |_, _, cx| content(&this, cx))
}

fn content(this: &Entity<Calculator>, cx: &mut App) -> Div {
    let calculator = this.read(cx);
    let entries = calculator.visible_entries(cx);
    let none_at_all = calculator.entries.is_empty();
    let mark = calculator.palette.selected_mark;
    let search = calculator.search.clone();
    let current = calculator.current.clone();
    let pending = calculator.pending_delete.clone();
    let rows: Vec<_> = entries
        .into_iter()
        .enumerate()
        .map(|(ix, entry)| {
            let selected = current.as_ref() == Some(&entry.id);
            let arming = pending.as_ref() == Some(&entry.id);
            row(this, ix, entry, selected.then_some(mark), arming, cx).into_any_element()
        })
        .collect();
    let empty = rows.is_empty();
    v_flex()
        .w(px(WIDTH))
        .gap_2()
        .child(
            Input::new(&search).small().rounded_full().prefix(
                Icon::new(IconName::Search)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            ),
        )
        .child(
            div()
                .id("calculations-list")
                .max_h(px(LIST_HEIGHT))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_0p5()
                .children(rows)
                .when(empty, |list| {
                    list.child(
                        div()
                            .px_2()
                            .py_1()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if none_at_all {
                                "All your calculations will appear here"
                            } else {
                                "No calculations match"
                            }),
                    )
                }),
        )
}

fn row(
    this: &Entity<Calculator>,
    ix: usize,
    entry: Entry,
    mark: Option<gpui::Hsla>,
    arming: bool,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let group = format!("calculation-{ix}");
    let (hover, muted, danger) = (theme.secondary_hover, theme.muted_foreground, theme.danger);
    let opening = this.clone();
    let deleting = this.clone();
    let (open_id, delete_id) = (entry.id.clone(), entry.id);
    let label = entry.title.clone().unwrap_or_else(|| "Empty".to_owned());
    h_flex()
        .id(("calculation", ix))
        .group(group.clone())
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(6.))
        .text_sm()
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(move |_, window, cx| {
            opening.update(cx, |calculator, cx| {
                calculator.open_calculation(&open_id, window, cx);
                calculator.set_list_open(false, window, cx);
            });
        })
        // The open calculation's mark; the space stays for the others, so
        // every label starts in one place.
        .child(
            div()
                .flex_none()
                .size(px(MARK_SIZE))
                .rounded_full()
                .when_some(mark, |dot, mark| dot.bg(mark)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(entry.title.is_none(), |label| label.text_color(muted))
                .child(label),
        )
        .child(
            div()
                .id(("delete", ix))
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .rounded(px(4.))
                .text_xs()
                .text_color(if arming { danger } else { muted })
                .when(!arming, |trash| {
                    trash
                        .opacity(0.)
                        .group_hover(group, |style| style.opacity(1.))
                })
                .hover(move |style| style.text_color(danger))
                // The press belongs to the trash, not to the row.
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    deleting.update(cx, |calculator, cx| {
                        calculator.delete_pressed(&delete_id, window, cx);
                    });
                })
                .when(arming, |trash| trash.child("Delete?"))
                .child(Icon::new(Glyph::Trash).xsmall()),
        )
}
