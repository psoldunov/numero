//! Resizing from the frame's edges: bands along each free edge that show a
//! resize cursor, and the press that starts the resize. As concierge's
//! `ui/window_frame.rs`.

use gpui::{
    AnyElement, CursorStyle, Decorations, Edges, IntoElement, MouseDownEvent, Pixels, Point,
    ResizeEdge, Size, Styled as _, Tiling, Window, div, px,
};

use super::{insets, no_text_selection};

/// How far either side of the frame's edge a press starts a resize.
const RESIZE_REACH: Pixels = px(4.);

/// Starts a resize when a press lands near a free edge of the frame.
pub(super) fn on_press(_: &MouseDownEvent, window: &mut Window, _: &mut gpui::App) {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return;
    };
    if let Some(edge) = resize_edge(window.mouse_position(), window.viewport_size(), tiling) {
        window.start_window_resize(edge);
    }
}

/// Bands along the frame's free edges that show a resize cursor. Corners
/// come last, so they win where two bands cross. They paint over the
/// content, so they claim the press before a text selection sees it.
pub(super) fn zones(size: Size<Pixels>, insets: Edges<Pixels>, tiling: Tiling) -> Vec<AnyElement> {
    let (left, top) = (insets.left, insets.top);
    let (right, bottom) = (size.width - insets.right, size.height - insets.bottom);
    let band = RESIZE_REACH + RESIZE_REACH;
    let across = right - left + band;
    let down = bottom - top + band;
    let zone = |edge: ResizeEdge, x: Pixels, y: Pixels, width: Pixels, height: Pixels| {
        no_text_selection(div())
            .absolute()
            .left(x - RESIZE_REACH)
            .top(y - RESIZE_REACH)
            .w(width)
            .h(height)
            .cursor(cursor(edge))
            .into_any_element()
    };
    let mut zones = Vec::new();
    if !tiling.top {
        zones.push(zone(ResizeEdge::Top, left, top, across, band));
    }
    if !tiling.bottom {
        zones.push(zone(ResizeEdge::Bottom, left, bottom, across, band));
    }
    if !tiling.left {
        zones.push(zone(ResizeEdge::Left, left, top, band, down));
    }
    if !tiling.right {
        zones.push(zone(ResizeEdge::Right, right, top, band, down));
    }
    let corners = [
        (ResizeEdge::TopLeft, left, top, !(tiling.top || tiling.left)),
        (
            ResizeEdge::TopRight,
            right,
            top,
            !(tiling.top || tiling.right),
        ),
        (
            ResizeEdge::BottomLeft,
            left,
            bottom,
            !(tiling.bottom || tiling.left),
        ),
        (
            ResizeEdge::BottomRight,
            right,
            bottom,
            !(tiling.bottom || tiling.right),
        ),
    ];
    for (edge, x, y, free) in corners {
        if free {
            zones.push(zone(edge, x, y, band, band));
        }
    }
    zones
}

const fn cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// The edge a press at `position` resizes from, in a window of `size`:
/// within [`RESIZE_REACH`] of a free edge of the frame, corners first.
fn resize_edge(position: Point<Pixels>, size: Size<Pixels>, tiling: Tiling) -> Option<ResizeEdge> {
    /// The edge each pair of sides names: rows by the vertical side (none,
    /// top, bottom), columns by the horizontal one (none, left, right).
    const EDGES: [[Option<ResizeEdge>; 3]; 3] = [
        [None, Some(ResizeEdge::Left), Some(ResizeEdge::Right)],
        [
            Some(ResizeEdge::Top),
            Some(ResizeEdge::TopLeft),
            Some(ResizeEdge::TopRight),
        ],
        [
            Some(ResizeEdge::Bottom),
            Some(ResizeEdge::BottomLeft),
            Some(ResizeEdge::BottomRight),
        ],
    ];
    let insets = insets(tiling);
    let vertical = side(
        position.y,
        (insets.top, size.height - insets.bottom),
        (tiling.top, tiling.bottom),
    )?;
    let horizontal = side(
        position.x,
        (insets.left, size.width - insets.right),
        (tiling.left, tiling.right),
    )?;
    EDGES
        .get(vertical)
        .and_then(|row| row.get(horizontal))
        .copied()
        .flatten()
}

/// Where `at` falls along one axis of the frame, which runs `from` one edge
/// `to` the other: 1 near the first edge, 2 near the second, 0 between.
/// `None` past either edge's reach. A tiled edge has no reach.
fn side(
    at: Pixels,
    (from, to): (Pixels, Pixels),
    (from_tiled, to_tiled): (bool, bool),
) -> Option<usize> {
    let near = |edge: Pixels, tiled: bool| {
        !tiled && at >= edge - RESIZE_REACH && at <= edge + RESIZE_REACH
    };
    if near(from, from_tiled) {
        Some(1)
    } else if near(to, to_tiled) {
        Some(2)
    } else if at >= from - RESIZE_REACH && at <= to + RESIZE_REACH {
        Some(0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use gpui::{point, size};

    use super::super::SHADOW_INSET;
    use super::*;

    const FREE: Tiling = Tiling {
        top: false,
        left: false,
        right: false,
        bottom: false,
    };

    #[test]
    fn a_press_on_the_frames_edge_resizes_from_it_and_corners_win() {
        let window = size(px(600.), px(400.));
        let inset = SHADOW_INSET;
        assert_eq!(
            resize_edge(point(inset, px(200.)), window, FREE),
            Some(ResizeEdge::Left)
        );
        assert_eq!(
            resize_edge(point(px(300.), inset), window, FREE),
            Some(ResizeEdge::Top)
        );
        assert_eq!(
            resize_edge(point(inset, inset), window, FREE),
            Some(ResizeEdge::TopLeft)
        );
        assert_eq!(
            resize_edge(point(px(600.) - inset, px(400.) - inset), window, FREE),
            Some(ResizeEdge::BottomRight)
        );
    }

    #[test]
    fn a_press_inside_or_in_the_shadow_does_not_resize() {
        let window = size(px(600.), px(400.));
        assert_eq!(resize_edge(point(px(300.), px(200.)), window, FREE), None);
        assert_eq!(resize_edge(point(px(2.), px(2.)), window, FREE), None);
    }
}
