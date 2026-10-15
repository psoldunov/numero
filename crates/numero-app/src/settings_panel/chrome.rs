//! The Preferences window's options and title bar. It has the calculator
//! window's frame (the frame's plugins decorate every window), filled
//! opaque in its own ground, its own title bar on Linux, and AppKit's
//! traffic lights inside it on macOS. The tabs sit in the middle of the
//! bar, where the title would be.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Bounds, Context, Decorations, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};

use super::controls::{Choice, choices};
use super::look::Look;
use super::{Preferences, Tab};

const TITLE: &str = "Numero Preferences";
const WIDTH: f32 = 480.;
const HEIGHT: f32 = 640.;
const BAR_HEIGHT: f32 = 44.;

pub(super) fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(WIDTH), px(HEIGHT)),
            cx,
        ))),
        window_min_size: Some(size(px(420.), px(360.))),
        app_id: Some(crate::app::APP_ID.to_owned()),
        titlebar: Some(titlebar_options()),
        window_background: WindowBackgroundAppearance::Transparent,
        app_owns_titlebar_drag: true,
        window_decorations: decorations(),
        ..WindowOptions::default()
    }
}

#[cfg(target_os = "macos")]
fn titlebar_options() -> TitlebarOptions {
    TitlebarOptions {
        title: Some(TITLE.into()),
        appears_transparent: true,
        traffic_light_position: Some(gpui::point(px(12.), px((BAR_HEIGHT - 14.) / 2.))),
    }
}

#[cfg(not(target_os = "macos"))]
fn titlebar_options() -> TitlebarOptions {
    TitlebarOptions {
        title: Some(TITLE.into()),
        ..TitlebarOptions::default()
    }
}

#[cfg(target_os = "macos")]
const fn decorations() -> Option<gpui::WindowDecorations> {
    None
}

#[cfg(not(target_os = "macos"))]
const fn decorations() -> Option<gpui::WindowDecorations> {
    Some(gpui::WindowDecorations::Client)
}

/// The tabs in the middle; a press elsewhere moves the window, and on
/// Linux a close button sits on the right.
pub(super) fn title_bar(
    preferences: &Preferences,
    window: &Window,
    cx: &mut Context<'_, Preferences>,
) -> impl IntoElement + use<> {
    let decorated = matches!(window.window_decorations(), Decorations::Client { .. });
    let look = Look::current(cx);
    let (muted, danger, danger_text) =
        (look.muted, cx.theme().danger, cx.theme().danger_foreground);
    let tabs = [(Tab::General, "General"), (Tab::Extensions, "Extensions")]
        .into_iter()
        .map(|(tab, label)| Choice {
            label: label.into(),
            selected: preferences.tab == tab,
            on_click: Box::new(cx.listener(move |this, _, _, cx| this.select_tab(tab, cx))),
        })
        .collect();
    h_flex()
        .id("preferences-title-bar")
        .h(px(BAR_HEIGHT))
        .flex_none()
        .px_3()
        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
        .child(div().w(px(24.)))
        .child(div().flex_1().flex().justify_center().child(
            choices("preferences-tab", tabs, &look).on_mouse_down(
                MouseButton::Left,
                |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                },
            ),
        ))
        .child(div().w(px(24.)).when(decorated, |slot| {
            slot.child(
                div()
                    .id("preferences-close")
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_color(muted)
                    .hover(move |style| style.bg(danger).text_color(danger_text))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(|_, window, _| window.remove_window())
                    .child(Icon::new(IconName::WindowClose).xsmall()),
            )
        }))
}
