//! The result column's width: Numi's `resultPaneWidth`, 120 px to begin
//! with, kept between a third and a half of the window
//! (`docs/numi-spec-ui.md` §1.4). Dragging the line between the text and
//! the results moves it; the width is saved when the drag ends.

use gpui::{Context, Pixels, px};

use super::Calculator;

/// A drag of the line between the text and the results.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DividerDrag {
    /// Where the pointer was pressed.
    start_x: Pixels,
    /// The width when it was pressed.
    start_width: f32,
    /// The width as dragged so far.
    pub(crate) width: f32,
}

/// `width` as a window `window_width` wide shows it.
pub(crate) fn clamped(width: f32, window_width: Pixels) -> Pixels {
    px(width).max(window_width / 3.).min(window_width / 2.)
}

impl Calculator {
    /// The result column's width in a window `window_width` wide.
    pub(crate) fn results_width(&self, window_width: Pixels) -> Pixels {
        let width = self
            .divider_drag
            .map_or(self.settings.result_pane_width, |drag| drag.width);
        clamped(width, window_width)
    }

    /// Starts dragging the line, pressed at `x`.
    pub(crate) fn start_divider_drag(
        &mut self,
        x: Pixels,
        window_width: Pixels,
        cx: &mut Context<'_, Self>,
    ) {
        let width = self.results_width(window_width) / px(1.);
        self.divider_drag = Some(DividerDrag {
            start_x: x,
            start_width: width,
            width,
        });
        cx.notify();
    }

    /// Follows the pointer, now at `x`: the column grows as it moves left.
    pub(crate) fn drag_divider(
        &mut self,
        x: Pixels,
        window_width: Pixels,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(drag) = &mut self.divider_drag else {
            return;
        };
        let wanted = drag.start_width + (drag.start_x - x) / px(1.);
        drag.width = clamped(wanted, window_width) / px(1.);
        cx.notify();
    }

    /// Ends the drag and keeps the width.
    pub(crate) fn end_divider_drag(&mut self, cx: &mut Context<'_, Self>) {
        let Some(drag) = self.divider_drag.take() else {
            return;
        };
        self.update_settings(|settings| settings.result_pane_width = drag.width, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_width_stays_between_a_third_and_a_half_of_the_window() {
        assert_eq!(clamped(120., px(370.)), px(370.) / 3.);
        assert_eq!(clamped(150., px(370.)), px(150.));
        assert_eq!(clamped(400., px(370.)), px(185.));
    }
}
