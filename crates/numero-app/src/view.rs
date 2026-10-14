//! How the calculator's window is laid out: the title bar, the editor and
//! the results column side by side with a hairline between them, which a
//! drag moves, then the footer with the settings, the grand total and
//! Export. The frame around it all, and the translucent background, come
//! from `frame`.

use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, MouseMoveEvent, ParentElement as _,
    Pixels, Render, ScrollWheelEvent, Styled as _, Window, div, px,
};
use gpui_component::input::Editor;

use crate::app::{ResetZoom, ZoomIn, ZoomOut};
use crate::calculator::{Calculator, Zoom};
use crate::follow::FollowEditorScroll;
use crate::results;

/// How wide the strip that takes a drag of the line between the columns is.
const GRIP_WIDTH: f32 = 6.;

impl Render for Calculator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let width = window.viewport_size().width;
        self.set_viewport_width(width);
        let line_height = self.line_height(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom(Zoom::In, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom(Zoom::Out, cx)))
            .on_action(cx.listener(|this, _: &ResetZoom, _, cx| this.zoom(Zoom::Reset, cx)))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.divider_drag.is_none() {
                    return;
                }
                if event.pressed_button == Some(MouseButton::Left) {
                    this.drag_divider(event.position.x, width, cx);
                } else {
                    this.end_divider_drag(cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_divider_drag(cx)),
            )
            .child(self.title_bar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(
                        div().flex_1().min_w_0().h_full().pl(px(4.)).child(
                            Editor::new(&self.editor)
                                .appearance(false)
                                .bordered(false)
                                .h_full()
                                .text_size(self.text_size()),
                        ),
                    )
                    // After the editor, so it follows the scroll the editor
                    // settled on this frame.
                    .child(self.results_pane(width, line_height, cx)),
            )
            .child(self.footer(cx))
    }
}

impl Calculator {
    fn results_pane(
        &self,
        window_width: Pixels,
        line_height: Pixels,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        div()
            .relative()
            .w(self.results_width(window_width))
            .border_l_1()
            .border_color(self.palette.separator)
            .flex_shrink_0()
            .h_full()
            .pl(px(8.))
            .pr(px(14.))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                this.scroll_editor(event, cx);
            }))
            .child(FollowEditorScroll::new(
                self.editor.clone(),
                results::column(self, line_height, cx),
            ))
            .child(divider_grip(window_width, cx))
    }
}

/// The strip over the line between the columns that a drag moves.
fn divider_grip(
    window_width: Pixels,
    cx: &mut Context<'_, Calculator>,
) -> impl IntoElement + use<> {
    div()
        .id("result-pane-divider")
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-GRIP_WIDTH / 2.))
        .w(px(GRIP_WIDTH))
        .cursor_col_resize()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.start_divider_drag(event.position.x, window_width, cx);
            }),
        )
}
