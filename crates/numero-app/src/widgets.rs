//! Small controls the window's chrome shares: the muted icon buttons in its
//! four corners, as Numi has them.

use gpui::{App, Styled as _};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

/// A small, muted icon button with no frame until the pointer is on it.
pub(crate) fn corner_button(
    id: &'static str,
    icon: impl Into<Icon>,
    tooltip: &'static str,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .icon(icon.into().text_color(cx.theme().muted_foreground))
        .tooltip(tooltip)
}
