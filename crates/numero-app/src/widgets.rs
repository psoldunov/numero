//! Small controls the window's chrome shares: the muted icon buttons in its
//! four corners, as Numi has them, and the chips the settings pick with.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, ClickEvent, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
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

/// One choice among a few, highlighted when `selected`.
pub(crate) fn chip(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (hover, selected_bg, ink, muted) = (
        theme.secondary_hover,
        theme.secondary_active,
        theme.foreground,
        theme.muted_foreground,
    );
    div()
        .id(id)
        .px(px(7.))
        .py(px(2.))
        .min_w(px(22.))
        .flex()
        .justify_center()
        .rounded(px(5.))
        .cursor_pointer()
        .text_color(muted)
        .hover(move |style| style.bg(hover))
        .when(selected, |chip| chip.bg(selected_bg).text_color(ink))
        .on_click(on_click)
        .child(label.into())
}

/// A label on the left and its control on the right.
pub(crate) fn row(label: &'static str, control: impl IntoElement, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(div().text_color(cx.theme().muted_foreground).child(label))
        .child(control)
}
