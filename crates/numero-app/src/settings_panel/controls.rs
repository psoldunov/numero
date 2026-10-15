//! The Preferences window's controls, in its own colours ([`Look`]). A
//! section is a title over one panel of rows; a row is a label, with a hint
//! under it when the label needs one, and its control on the right.

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, Focusable as _, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, TextAlign, Window, div, px, transparent_black,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::switch::Switch;
use gpui_component::{Sizable as _, h_flex, v_flex};

use super::look::Look;

/// How far a panel's text sits from its edge. Section titles line up with
/// it, so every text in the window starts on one of two edges.
pub(super) const INSET: Pixels = px(12.);
/// Between two sections: clearly more than anything inside one.
pub(super) const SECTION_GAP: Pixels = px(22.);
const PANEL_RADIUS: Pixels = px(10.);
const TRACK_RADIUS: Pixels = px(7.);
/// The height of a set of choices, a typed field and a step button.
const CONTROL_HEIGHT: Pixels = px(26.);

pub(super) type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A section: its title, then its panel.
pub(super) fn section(title: &'static str, panel: impl IntoElement, look: &Look) -> Div {
    v_flex()
        .gap(px(6.))
        .child(
            div()
                .pl(INSET)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(look.ink)
                .child(title),
        )
        .child(panel)
}

/// The panel a section's rows sit on.
pub(super) fn panel(look: &Look) -> Div {
    v_flex()
        .px(INSET)
        .py(px(4.))
        .rounded(PANEL_RADIUS)
        .bg(look.panel)
}

/// `label`, with `hint` under it, on the left and `control` on the right.
pub(super) fn row(
    label: &'static str,
    hint: Option<AnyElement>,
    control: impl IntoElement,
    look: &Look,
) -> Div {
    h_flex()
        .min_h(px(44.))
        .py(px(7.))
        .gap_4()
        .justify_between()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(1.))
                .child(div().text_color(look.ink).child(label))
                .children(hint),
        )
        .child(div().flex_none().child(control))
}

/// A line of small text under a label, or in a panel of its own.
pub(super) fn hint(text: impl Into<SharedString>, color: Hsla) -> AnyElement {
    div()
        .text_xs()
        .text_color(color)
        .child(text.into())
        .into_any_element()
}

/// One choice in a [`choices`] row.
pub(super) struct Choice {
    pub label: SharedString,
    pub selected: bool,
    pub on_click: OnClick,
}

/// A row of choices on a track, the picked one raised off it.
pub(super) fn choices(id: &'static str, choices: Vec<Choice>, look: &Look) -> Div {
    let Look {
        selected,
        ink,
        muted,
        ..
    } = *look;
    h_flex()
        .p(px(2.))
        .gap(px(2.))
        .rounded(TRACK_RADIUS)
        .bg(look.track)
        .children(choices.into_iter().enumerate().map(move |(ix, choice)| {
            let segment = div()
                .id((id, ix))
                .h(CONTROL_HEIGHT - px(4.))
                .min_w(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.));
            let segment = if choice.selected {
                segment.bg(selected).text_color(ink)
            } else {
                segment
                    .text_color(muted)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(ink))
                    .on_click(choice.on_click)
            };
            segment.child(choice.label)
        }))
}

/// A field to type a number in, `width` wide, ringed in the accent while
/// it has the focus.
pub(super) fn number_field(
    state: &Entity<InputState>,
    width: Pixels,
    look: &Look,
    window: &Window,
    cx: &App,
) -> Div {
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .w(width)
        .h(CONTROL_HEIGHT)
        .flex()
        .items_center()
        .rounded(TRACK_RADIUS)
        .bg(look.track)
        .border_2()
        .border_color(if focused {
            look.accent
        } else {
            transparent_black()
        })
        .text_color(look.ink)
        .child(
            Input::new(state)
                .appearance(false)
                .small()
                .text_align(TextAlign::Center),
        )
}

/// A square button with a sign on it, such as a stepper's `−`; drawn but
/// inert when `on_click` is `None`.
pub(super) fn step_button(
    id: impl Into<ElementId>,
    sign: &'static str,
    on_click: Option<OnClick>,
    look: &Look,
) -> Stateful<Div> {
    let button = div()
        .id(id)
        .size(CONTROL_HEIGHT)
        .flex()
        .items_center()
        .justify_center()
        .rounded(TRACK_RADIUS)
        .bg(look.button);
    match on_click {
        Some(on_click) => {
            let hover = look.button_hover;
            button
                .text_color(look.ink)
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_click(on_click)
        }
        None => button.text_color(look.muted.opacity(0.5)),
    }
    .child(sign)
}

/// A button in the panel's colours.
pub(super) fn button(id: &'static str, label: &'static str, look: &Look, cx: &App) -> Button {
    Button::new(id).small().label(label).custom(
        ButtonCustomVariant::new(cx)
            .color(look.button)
            .foreground(look.ink)
            .hover(look.button_hover)
            .active(look.button_hover),
    )
}

/// The one button a moment calls for, in the accent.
pub(super) fn primary_button(
    id: &'static str,
    label: &'static str,
    look: &Look,
    cx: &App,
) -> Button {
    let pressed = look.accent.opacity(0.85);
    Button::new(id).small().label(label).custom(
        ButtonCustomVariant::new(cx)
            .color(look.accent)
            .foreground(look.accent_ink)
            .hover(pressed)
            .active(pressed),
    )
}

/// A switch, green while on.
pub(super) fn switch(
    id: &'static str,
    on: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
    look: &Look,
) -> Switch {
    Switch::new(id)
        .checked(on)
        .color(look.accent)
        .on_click(on_click)
}

/// A path or other long text that must stay on one line, shortened in its
/// middle, on a track.
pub(super) fn path_field(text: impl Into<SharedString>, look: &Look) -> Div {
    div()
        .w_full()
        .min_w_0()
        .px(px(10.))
        .py(px(5.))
        .rounded(TRACK_RADIUS)
        .bg(look.track)
        .text_color(look.ink)
        .child(
            div()
                .w_full()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis_middle()
                .child(text.into()),
        )
}
