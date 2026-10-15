//! The window's frame: rounded corners, a soft shadow, a translucent
//! background with the desktop blurred behind it, and the edges it resizes
//! from. The same approach as concierge's `ui/window_frame.rs`, for one
//! window.
//!
//! On Linux the window draws its own decorations, so the app draws these.
//! gpui-component wraps every window in its `window_border`, whose corners
//! are square and whose 20 px shadow band shows black on an opaque surface.
//! Root plugins decorate the surface in the order they were registered:
//! [`Capture`], registered before gpui-component, takes the surface before
//! `window_border` sees it; [`Frame`], registered after, drops
//! `window_border`'s wrapper and frames the surface itself.
//!
//! On macOS AppKit draws the frame, corners and shadow; [`Frame`] only fills
//! the window with the translucent background and asks for the blur.
//!
//! A window whose view asked for it with [`fill_opaque`] gets the same
//! frame filled with a solid colour instead, and no blur.

mod resize;

use std::any::TypeId;
use std::cell::{Cell, RefCell};

use gpui::base::{Root, RootPlugin};
use gpui::{
    AnyElement, App, Bounds, BoxShadow, Context, Corners, Decorations, Div, Edges, Global, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement as _, Pixels, Point, Render,
    Stateful, Styled, Tiling, Window, div, hsla, point, px, size,
};
use gpui_component::{ActiveTheme as _, GlobalState};

use crate::blur::{self, Blur};
use crate::theme::Palette;

/// How far the visible frame sits inside the window's surface, leaving room
/// for its shadow.
const SHADOW_INSET: Pixels = px(24.);

/// The soft shadow's blur. GPUI draws a shadow out to three times its blur
/// and cuts it off at the surface's edge, so blur, offset and spread stay
/// within [`SHADOW_INSET`].
const SHADOW_BLUR: Pixels = px(7.);

/// How round the frame's corners are.
pub(crate) const RADIUS: Pixels = px(12.);

/// The nearly opaque rim inside the frame's edge, one logical pixel: as far
/// as the blur region's whole-pixel steps stray from a corner's curve.
const RIM_WIDTH: Pixels = px(1.);
const RIM_OPACITY: f32 = 0.85;

/// Registers the plugin that takes the window's surface. Call it before
/// `gpui::init`, then [`register_frame`] after.
pub(crate) fn register_capture(cx: &mut App) {
    Root::register_plugin(cx, Capture::new);
}

/// Registers the plugin that draws the frame. Call it after `gpui::init`.
pub(crate) fn register_frame(cx: &mut App) {
    Root::register_plugin(cx, Frame::new);
}

/// Joins the compositor's background effects, so [`see_through`] knows
/// before the first frame. Call it while building the window's view.
pub(crate) fn connect(window: &Window, cx: &mut App) {
    blur::connect(window, cx);
}

/// The colour an opaque window is filled with, asked each frame so it
/// follows the theme.
type Ground = fn(&App) -> Hsla;

/// The views whose windows are filled opaque, each with its colour.
#[derive(Default)]
struct Opaque(Vec<(TypeId, Ground)>);

impl Global for Opaque {}

/// Fills every window whose view is a `V` with `ground` rather than the
/// translucent background, and blurs nothing behind it.
pub(crate) fn fill_opaque<V: 'static>(ground: Ground, cx: &mut App) {
    let view = TypeId::of::<V>();
    let opaque = cx.default_global::<Opaque>();
    if !opaque.0.iter().any(|(kind, _)| *kind == view) {
        opaque.0.push((view, ground));
    }
}

/// The solid colour a window showing a view of type `view` is filled with,
/// if [`fill_opaque`] asked for one.
fn opaque_ground(view: TypeId, cx: &App) -> Option<Hsla> {
    let (_, ground) = cx
        .try_global::<Opaque>()?
        .0
        .iter()
        .find(|(kind, _)| *kind == view)?;
    Some(ground(cx))
}

/// `color` as the translucent window paints it: as it is where the desktop
/// blurs what shows through, and opaque where it doesn't, so text stays
/// readable whatever is behind the window.
pub(crate) fn see_through(color: Hsla, cx: &App) -> Hsla {
    see_through_when(color, blur::available(cx))
}

fn see_through_when(color: Hsla, blurred: bool) -> Hsla {
    if blurred {
        color
    } else {
        Hsla { a: 1., ..color }
    }
}

/// `element`, with a left press on it starting no text selection: a press
/// that moves or resizes the window hands its release to the compositor,
/// so a selection it started would never end.
pub(crate) fn no_text_selection<E: InteractiveElement>(element: E) -> E {
    element.on_mouse_down(MouseButton::Left, |_, _, cx| {
        GlobalState::suppress_text_selection(cx);
    })
}

/// Holds the window's surface between the two plugins' turns. Empty
/// between frames.
struct Capture {
    surface: RefCell<Option<AnyElement>>,
}

impl Capture {
    const fn new(_: &mut Window, _: &mut Context<'_, Self>) -> Self {
        Self {
            surface: RefCell::new(None),
        }
    }
}

impl Render for Capture {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl RootPlugin for Capture {
    fn decorate(
        &self,
        surface: AnyElement,
        _: &Root,
        _: &mut Window,
        _: &mut App,
    ) -> impl IntoElement {
        self.surface.replace(Some(surface));
        // What `window_border` wraps; `Frame` throws it away.
        div().into_any_element()
    }
}

/// Frames the surface `Capture` took and blurs what is behind the frame.
struct Frame {
    /// The window's blur, once attached; `None` where it can't have one.
    blur: RefCell<Option<Blur>>,
    /// Whether attaching was tried: a surface takes one background effect.
    tried_blur: Cell<bool>,
}

impl Frame {
    const fn new(_: &mut Window, _: &mut Context<'_, Self>) -> Self {
        Self {
            blur: RefCell::new(None),
            tried_blur: Cell::new(false),
        }
    }

    fn blur(&self, window: &mut Window, cx: &mut App) {
        if blur::refresh(cx) {
            // The desktop started or stopped blurring; `see_through`
            // changes with it.
            window.request_animation_frame();
        }
        if !self.tried_blur.replace(true) {
            self.blur.replace(Blur::attach(window, cx));
        }
        if let Some(blur) = self.blur.borrow_mut().as_mut() {
            let (bounds, radii) = shape(window);
            blur.set(bounds, radii, cx);
        }
    }
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl RootPlugin for Frame {
    fn style(&self, surface: &mut Stateful<Div>, _: &mut Window, _: &mut App) {
        // The frame paints the background, rounded; the surface's own
        // square one would show past the corners.
        surface.style().background = Some(gpui::transparent_black().into());
    }

    fn decorate(
        &self,
        surface: AnyElement,
        root: &Root,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let captured = root
            .plugin::<Capture>()
            .and_then(|capture| capture.read(cx).surface.take());
        let Some(content) = captured else {
            return surface;
        };
        let ground = opaque_ground(root.view().entity_type(), cx);
        if ground.is_none() {
            self.blur(window, cx);
        }
        frame(content, ground, window, cx)
    }
}

/// Where the frame shows on `window` and how round its corners are,
/// measured from the window geometry's corner, which is where `KWin` reads
/// a blur region from.
fn shape(window: &Window) -> (Bounds<Pixels>, Corners<Pixels>) {
    let surface = window.viewport_size();
    match window.window_decorations() {
        Decorations::Server => (Bounds::new(Point::default(), surface), Corners::all(px(0.))),
        Decorations::Client { tiling } => {
            let insets = insets(tiling);
            let frame = Bounds::new(
                Point::default(),
                size(
                    surface.width - insets.left - insets.right,
                    surface.height - insets.top - insets.bottom,
                ),
            );
            (frame, corners(tiling))
        }
    }
}

/// The frame's corners: round where neither side beside it is tiled.
fn corners(tiling: Tiling) -> Corners<Pixels> {
    let corner = |tiled: bool| if tiled { px(0.) } else { RADIUS };
    Corners {
        top_left: corner(tiling.top || tiling.left),
        top_right: corner(tiling.top || tiling.right),
        bottom_right: corner(tiling.bottom || tiling.right),
        bottom_left: corner(tiling.bottom || tiling.left),
    }
}

/// `content` in the frame: inset for the shadow on every side that isn't
/// tiled, with a hairline border and rounded corners where it floats free.
/// Filled with `ground` when there is one, else translucent.
fn frame(content: AnyElement, ground: Option<Hsla>, window: &mut Window, cx: &App) -> AnyElement {
    let background = ground.unwrap_or_else(|| see_through(Palette::current(cx).background, cx));
    let Decorations::Client { tiling } = window.window_decorations() else {
        return div()
            .size_full()
            .bg(background)
            .child(content)
            .into_any_element();
    };
    // Kept even when every side is tiled: clearing it makes the first resize
    // after a restore count the shadow twice.
    window.set_client_inset(SHADOW_INSET);
    let insets = insets(tiling);
    let corners = corners(tiling);
    let size = window.viewport_size();
    let shadows = shadows(
        background,
        cx.theme().is_dark(),
        window.is_window_active(),
        tiling.is_tiled(),
    );
    div()
        .id("window-frame")
        .size_full()
        .pt(insets.top)
        .pb(insets.bottom)
        .pl(insets.left)
        .pr(insets.right)
        .on_mouse_down(MouseButton::Left, resize::on_press)
        .child(
            hairline_edges(
                div()
                    .size_full()
                    .overflow_hidden()
                    .bg(background)
                    .border_color(cx.theme().window_border)
                    .rounded_tl(corners.top_left)
                    .rounded_tr(corners.top_right)
                    .rounded_bl(corners.bottom_left)
                    .rounded_br(corners.bottom_right)
                    .shadow(shadows),
                tiling,
                window.scale_factor(),
            )
            .child(content),
        )
        .children(resize::zones(size, insets, tiling))
        .into_any_element()
}

/// `frame` with a border one device pixel wide on each side that isn't
/// tiled. GPUI's border methods only come in whole logical pixels, which a
/// fractional `scale` smears across two device pixels.
fn hairline_edges(mut frame: Div, tiling: Tiling, scale: f32) -> Div {
    let hairline = px(1. / scale);
    let edge = |tiled: bool| (!tiled).then(|| hairline.into());
    let widths = &mut frame.style().border_widths;
    widths.top = edge(tiling.top);
    widths.right = edge(tiling.right);
    widths.bottom = edge(tiling.bottom);
    widths.left = edge(tiling.left);
    frame
}

/// The space between the window's surface and the visible frame.
fn insets(tiling: Tiling) -> Edges<Pixels> {
    let side = |tiled: bool| if tiled { px(0.) } else { SHADOW_INSET };
    Edges {
        top: side(tiling.top),
        right: side(tiling.right),
        bottom: side(tiling.bottom),
        left: side(tiling.left),
    }
}

/// The frame's shadows: where it floats free, a soft drop shadow and a
/// tight one under it, fainter while the window is in the background; and
/// always a thin, nearly opaque rim of `background` inside its edge, which
/// covers the blur region's stair-steps at the rounded corners.
///
/// GPUI fills a drop shadow under its element too, and the translucent
/// background shows it through, so the drop shadows stay faint.
fn shadows(background: Hsla, dark: bool, active: bool, tiled: bool) -> Vec<BoxShadow> {
    let rim = BoxShadow {
        color: Hsla {
            a: RIM_OPACITY,
            ..background
        },
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: RIM_WIDTH,
        inset: true,
    };
    if tiled {
        return vec![rim];
    }
    let strength = match (dark, active) {
        (true, true) => 0.26,
        (true, false) => 0.16,
        (false, true) => 0.12,
        (false, false) => 0.07,
    };
    vec![
        BoxShadow {
            color: hsla(0., 0., 0., strength),
            offset: point(px(0.), px(3.)),
            blur_radius: SHADOW_BLUR,
            spread_radius: px(-3.),
            inset: false,
        },
        BoxShadow {
            color: hsla(0., 0., 0., strength * 0.3),
            offset: point(px(0.), px(0.5)),
            blur_radius: px(1.),
            spread_radius: px(0.),
            inset: false,
        },
        rim,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_blur_the_background_is_opaque() {
        let color = hsla(0.5, 0.1, 0.9, 0.8);
        assert_eq!(see_through_when(color, true), color);
        assert_eq!(see_through_when(color, false).a, 1.);
    }

    #[gpui::test]
    fn only_views_that_asked_are_filled_opaque(cx: &mut gpui::TestAppContext) {
        let solid = hsla(0., 0., 0.2, 1.);
        cx.update(|cx| {
            assert_eq!(opaque_ground(TypeId::of::<u8>(), cx), None);
            fill_opaque::<u8>(|_| hsla(0., 0., 0.2, 1.), cx);
            fill_opaque::<u8>(|_| hsla(0., 0., 0.9, 1.), cx);
            assert_eq!(opaque_ground(TypeId::of::<u8>(), cx), Some(solid));
            assert_eq!(opaque_ground(TypeId::of::<u16>(), cx), None);
        });
    }

    #[test]
    fn a_tiled_side_has_no_inset_and_squares_its_corners() {
        let tiling = Tiling {
            top: true,
            left: true,
            right: false,
            bottom: false,
        };
        let insets = insets(tiling);
        assert_eq!((insets.top, insets.left), (px(0.), px(0.)));
        assert_eq!((insets.bottom, insets.right), (SHADOW_INSET, SHADOW_INSET));
        let corners = corners(tiling);
        assert_eq!(corners.top_left, px(0.));
        assert_eq!(corners.top_right, px(0.));
        assert_eq!(corners.bottom_right, RADIUS);
    }

    #[test]
    fn the_drop_shadows_fit_inside_the_inset() {
        for shadow in shadows(hsla(0., 0., 1., 0.8), true, true, false) {
            if !shadow.inset {
                let reach = shadow.blur_radius * 3. + shadow.offset.y.abs() + shadow.spread_radius;
                assert!(reach <= SHADOW_INSET, "{reach:?}");
            }
        }
    }
}
