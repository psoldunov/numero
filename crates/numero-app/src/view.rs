//! How the calculator's window is laid out: the title bar, the editor and
//! the results column side by side with no line between them, then the
//! footer with the settings and Export. The frame around it all, and the
//! translucent background, come from `frame`.

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    ScrollWheelEvent, Styled as _, Window, div, px,
};
use gpui_component::input::Editor;

use crate::app::{ResetZoom, ZoomIn, ZoomOut};
use crate::calculator::{Calculator, Zoom};
use crate::follow::FollowEditorScroll;
use crate::results;

/// The result pane's width: 120 px, kept between a third and a half of
/// the window (`docs/numi-spec-ui.md` §1.4).
const RESULTS_WIDTH: f32 = 120.;

fn results_width(window_width: Pixels) -> Pixels {
    px(RESULTS_WIDTH)
        .max(window_width / 3.)
        .min(window_width / 2.)
}

impl Render for Calculator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let line_height = self.line_height(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.zoom(Zoom::In, window, cx)))
            .on_action(
                cx.listener(|this, _: &ZoomOut, window, cx| this.zoom(Zoom::Out, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ResetZoom, window, cx| this.zoom(Zoom::Reset, window, cx)),
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
                    .child(
                        div()
                            .w(results_width(window.viewport_size().width))
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
                            )),
                    ),
            )
            .child(self.footer(cx))
    }
}
