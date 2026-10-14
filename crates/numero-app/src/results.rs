//! The results column: each line's value, right-aligned and level with the
//! line, in the theme's result green. Under the pointer a result sits on a
//! green pill (`docs/numi-spec-ui.md` §4.2); a click copies it, and the
//! footer says so.
//!
//! The column is laid out with line 0 at its top and as tall as the
//! document; `FollowEditorScroll` moves it with the editor.

use gpui::{
    ClickEvent, Context, Div, InteractiveElement as _, ParentElement as _, Pixels, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::ActiveTheme as _;

use crate::calculator::Calculator;
use crate::document::ResultRow;

/// The column for `calculator`'s current results.
pub(crate) fn column(
    calculator: &Calculator,
    line_height: Pixels,
    cx: &mut Context<'_, Calculator>,
) -> Div {
    let lines = calculator.results.len().max(1);
    div()
        .relative()
        .w_full()
        .flex_shrink_0()
        .h(line_height * lines as f32)
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(calculator.text_size())
        .line_height(line_height)
        .children(
            calculator
                .rows
                .iter()
                .map(|row| cell(calculator, row, line_height, cx)),
        )
}

fn cell(
    calculator: &Calculator,
    row: &ResultRow,
    line_height: Pixels,
    cx: &mut Context<'_, Calculator>,
) -> Stateful<Div> {
    let palette = calculator.palette;
    let line = row.line;
    let label: SharedString = row.display.clone().into();
    div()
        .id(("result", line))
        .absolute()
        .top(line_height * line as f32)
        .right_0()
        .max_w_full()
        .h(line_height)
        .flex()
        .items_center()
        .px(px(6.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_color(palette.result)
        .hover(move |style| {
            style
                .bg(palette.result_hover)
                .text_color(palette.result_hover_text)
        })
        .active(move |style| {
            style
                .bg(palette.result_pressed)
                .text_color(palette.result_hover_text)
        })
        .on_hover(
            cx.listener(move |this, hovered: &bool, _, cx| this.hover_result(line, *hovered, cx)),
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_result(line, cx)))
        .child(div().truncate().child(label))
}
