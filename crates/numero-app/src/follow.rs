//! An element that scrolls its child with the editor, so each result stays
//! level with its line.
//!
//! The editor's scroll handle is private to gpui-base, but its offset and the
//! bounds of its text area are public. The child is laid out with line 0 at
//! its top; while it prepaints, this element shifts it to where the editor
//! drew line 0 this frame. It must come after the editor in the tree: the
//! editor settles its scroll offset (following the cursor, clamping) in its
//! own prepaint, so reading it here, rather than in `render`, keeps the two
//! columns in step on the same frame.

use std::panic::Location;

use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, Entity, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, point, px, relative,
};
use gpui_component::input::EditorState;

/// Draws `child` clipped to this element's box and moved by the editor's
/// vertical scroll.
pub(crate) struct FollowEditorScroll {
    editor: Entity<EditorState>,
    child: AnyElement,
}

impl FollowEditorScroll {
    pub(crate) fn new(editor: Entity<EditorState>, child: impl IntoElement) -> Self {
        Self {
            editor,
            child: child.into_any_element(),
        }
    }

    /// How far down from this element's top the editor's line 0 sits now.
    fn shift(&self, bounds: Bounds<Pixels>, cx: &App) -> Pixels {
        let editor = self.editor.read(cx);
        let text_top = editor.input_bounds().origin.y;
        // Before the editor's first paint its text area is unknown; line 0
        // then sits at the top.
        if text_top == px(0.) {
            return editor.scroll_offset().y;
        }
        text_top + editor.scroll_offset().y - bounds.origin.y
    }
}

impl IntoElement for FollowEditorScroll {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FollowEditorScroll {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let child = self.child.request_layout(window, cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [child], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let shift = self.shift(bounds, cx);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.with_element_offset(point(px(0.), shift), |window| {
                self.child.prepaint(window, cx);
            });
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx);
        });
    }
}
