//! The window's own title bar: the app's name in the middle; on the right
//! New calculation, the list of calculations and, on Linux, the window's
//! controls. As concierge's `ui/main_window/title_bar.rs`.
//!
//! It moves the window the way gpui-component's `TitleBar` does: a press
//! starts a move once the pointer moves, so a double click still arrives,
//! and maximizes. A right press opens the desktop's window menu.
//!
//! On macOS the controls are the system's traffic lights, which AppKit
//! draws inside this bar at [`TRAFFIC_LIGHTS`] (concierge's ADR 0063). The
//! bar leaves them room and draws no controls of its own, and a double
//! click does what the system setting says.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, Decorations, Div, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Stateful, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};

use crate::calculations_list;
use crate::calculator::Calculator;
use crate::frame::no_text_selection;
use crate::widgets::corner_button;

/// The title bar's height.
const HEIGHT: f32 = 34.;

/// The side of AppKit's traffic-light buttons, and the width of all three
/// with their gaps.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHT_SIZE: f32 = 14.;
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_WIDTH: f32 = 52.;

/// Where AppKit puts the traffic lights: as far in from the left as the
/// bar's padding, centred on its height.
#[cfg(target_os = "macos")]
pub(crate) const TRAFFIC_LIGHTS: gpui::Point<gpui::Pixels> = gpui::Point {
    x: px(12.),
    y: px((HEIGHT - TRAFFIC_LIGHT_SIZE) / 2.),
};

/// `bar` with its padding on the left: past the traffic lights on macOS.
#[cfg(target_os = "macos")]
fn leading_padding<E: Styled>(bar: E) -> E {
    bar.pl(TRAFFIC_LIGHTS.x * 2. + px(TRAFFIC_LIGHTS_WIDTH))
}

#[cfg(not(target_os = "macos"))]
fn leading_padding<E: Styled>(bar: E) -> E {
    bar.pl(px(20.))
}

/// A window control's side.
const CONTROL_SIZE: f32 = 24.;

impl Calculator {
    pub(crate) fn title_bar(&self, window: &Window, cx: &Context<'_, Self>) -> impl IntoElement {
        let decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        leading_padding(no_text_selection(h_flex().id("title-bar")))
            .h(px(HEIGHT))
            .flex_none()
            .gap_2()
            .pr_2()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.moving_window = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.moving_window = false),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, _| this.moving_window = false))
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.moving_window {
                    this.moving_window = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    double_click(window);
                }
            })
            .when(decorated, |bar| {
                bar.on_mouse_down(MouseButton::Right, |event, window, _| {
                    window.show_window_menu(event.position);
                })
            })
            // Centred in the space the buttons leave, so it never runs under
            // them in a narrow window with Linux's window controls.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(self.palette.title)
                    .child(div().truncate().child("Numero")),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_0p5()
                    // A press here belongs to the buttons, not to the move.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        corner_button("new-calculation", IconName::Plus, "New calculation", cx)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_list_open(false, window, cx);
                                this.new_calculation(window, cx);
                            })),
                    )
                    .child(calculations_list::popover(self, cx)),
            )
            .when(decorated, |bar| bar.child(window_controls(window, cx)))
    }
}

/// Minimize and maximize where the desktop offers them, and close.
fn window_controls(window: &Window, cx: &App) -> impl IntoElement {
    let offered = window.window_controls();
    h_flex()
        .flex_none()
        .gap_1p5()
        .when(offered.minimize, |controls| {
            controls.child(control(
                "minimize",
                IconName::WindowMinimize,
                false,
                |window, _| {
                    window.minimize_window();
                },
                cx,
            ))
        })
        .when(offered.maximize, |controls| {
            let icon = if window.is_maximized() {
                IconName::WindowRestore
            } else {
                IconName::WindowMaximize
            };
            controls.child(control(
                "maximize",
                icon,
                false,
                |window, _| window.zoom_window(),
                cx,
            ))
        })
        .child(control(
            "close",
            IconName::WindowClose,
            true,
            |window, cx| {
                crate::calculator::main_window::save(cx);
                window.remove_window();
            },
            cx,
        ))
}

/// What a double click on the title bar does: maximize on Linux, and on
/// macOS what the system setting says.
fn double_click(window: &Window) {
    if cfg!(target_os = "macos") {
        window.titlebar_double_click();
    } else {
        window.zoom_window();
    }
}

/// A title bar control that runs `press`. Close turns red under the
/// pointer; the others only light up.
fn control(
    id: &'static str,
    icon: impl Into<Icon>,
    close: bool,
    press: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (hover, hover_text) = if close {
        (theme.danger, theme.danger_foreground)
    } else {
        (theme.secondary_hover, theme.foreground)
    };
    div()
        .id(id)
        .size(px(CONTROL_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius)
        .text_color(theme.muted_foreground)
        .hover(move |style| style.bg(hover).text_color(hover_text))
        .active(|style| style.opacity(0.8))
        // The press belongs to the control, not to the title bar's move.
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            press(window, cx);
        })
        .child(Icon::new(icon).xsmall())
}
