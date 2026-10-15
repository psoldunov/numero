//! The Preferences window, as Numi's (`docs/numi-spec-ui.md` §1.1): a
//! General tab and an Extensions tab. Every change applies at once and is
//! saved; the settings themselves live with the calculator, which this
//! window changes through `Calculator::update_settings`.
//!
//! - `chrome`: the window's options and its title bar, which holds the tabs.
//! - `look` and `controls`: the window's colours, and the sections, rows
//!   and controls drawn in them.
//! - `general`: the General tab, its theme, font size and window sections.
//! - `formatting`: precision, the separators and the CSS unit sizes.
//! - `recorder`: the global shortcut's recorder.
//! - `folder`: where the calculations are kept.
//! - `extensions`: the Extensions tab.

mod chrome;
mod controls;
mod extensions;
mod folder;
mod formatting;
mod general;
mod look;
mod recorder;

use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, Focusable, Global,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Subscription,
    WeakEntity, Window, div, px,
};
use gpui_component::input::InputState;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::v_flex;

use self::look::Look;
use crate::calculator::Calculator;
use crate::calculator::main_window;
use crate::settings::Settings;

/// The tabs, in Numi's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    General,
    Extensions,
}

/// The open Preferences window, so a second "Preferences…" brings it
/// forward rather than opening another.
struct PreferencesWindow(AnyWindowHandle);

impl Global for PreferencesWindow {}

/// Brings the Preferences window forward, opening it if it isn't open.
pub(crate) fn open(cx: &mut App) {
    if let Some(PreferencesWindow(window)) = cx.try_global::<PreferencesWindow>()
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let Some(calculator) = main_window::calculator(cx) else {
        tracing::warn!("no calculator to show the preferences of");
        return;
    };
    crate::frame::fill_opaque::<Preferences>(Look::ground, cx);
    let opened = gpui::open_window(chrome::window_options(cx), cx, |window, cx| {
        crate::frame::connect(window, cx);
        cx.new(|cx| Preferences::new(calculator.downgrade(), window, cx))
    });
    match opened {
        Ok((window, _)) => {
            cx.set_global(PreferencesWindow(window));
            cx.activate(true);
        }
        Err(error) => tracing::error!(%error, "couldn't open the preferences"),
    }
}

pub(crate) struct Preferences {
    calculator: WeakEntity<Calculator>,
    pub(crate) tab: Tab,
    /// The typed fields: precision, and the ppi and em sizes.
    precision: Entity<InputState>,
    ppi: Entity<InputState>,
    em: Entity<InputState>,
    /// Whether Numero opens at login, as the system said last.
    launch_at_login: bool,
    /// Why the last change to it failed.
    login_error: Option<String>,
    recorder: recorder::Recorder,
    /// The calculations folder's last error, or what moving them did.
    folder_status: Option<String>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Preferences {
    pub(crate) fn new(
        calculator: WeakEntity<Calculator>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let settings = calculator
            .read_with(cx, |calculator, _| calculator.settings.clone())
            .unwrap_or_default();
        let field = |value: String, window: &mut Window, cx: &mut Context<'_, Self>| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let precision = field(settings.precision.to_string(), window, cx);
        let ppi = field(formatting::number(settings.ppi), window, cx);
        let em = field(formatting::number(settings.em_px), window, cx);
        let mut subscriptions = vec![
            cx.subscribe_in(&precision, window, Self::on_precision_event),
            cx.subscribe_in(&ppi, window, Self::on_size_event),
            cx.subscribe_in(&em, window, Self::on_size_event),
        ];
        if let Some(calculator) = calculator.upgrade() {
            subscriptions.push(cx.observe(&calculator, |_, _, cx| cx.notify()));
        }
        let focus = cx.focus_handle();
        subscriptions.push(cx.on_blur(&focus, window, |this, _, cx| {
            this.recorder.stop(cx);
        }));
        Self {
            calculator,
            tab: Tab::General,
            precision,
            ppi,
            em,
            launch_at_login: crate::system::launch_at_login(),
            login_error: None,
            recorder: recorder::Recorder::default(),
            folder_status: None,
            focus,
            _subscriptions: subscriptions,
        }
    }

    /// The settings as they stand.
    pub(crate) fn settings(&self, cx: &App) -> Settings {
        self.calculator
            .read_with(cx, |calculator, _| calculator.settings.clone())
            .unwrap_or_default()
    }

    /// Changes the settings, which applies and saves them.
    pub(crate) fn update(&self, change: impl FnOnce(&mut Settings), cx: &mut Context<'_, Self>) {
        let updated = self.calculator.update(cx, |calculator, cx| {
            calculator.update_settings(change, cx);
        });
        if updated.is_err() {
            tracing::warn!("the calculator is gone; the change is lost");
        }
    }

    /// Turns opening at login on or off, through the system.
    pub(crate) fn set_launch_at_login(&mut self, enabled: bool, cx: &mut Context<'_, Self>) {
        match crate::system::set_launch_at_login(enabled) {
            Ok(()) => self.login_error = None,
            Err(error) => {
                tracing::error!(%error, enabled, "couldn't change opening at login");
                self.login_error = Some(error);
            }
        }
        self.launch_at_login = crate::system::launch_at_login();
        cx.notify();
    }

    pub(crate) fn select_tab(&mut self, tab: Tab, cx: &mut Context<'_, Self>) {
        self.tab = tab;
        self.recorder.stop(cx);
        cx.notify();
    }
}

impl Focusable for Preferences {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Preferences {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let body = match self.tab {
            Tab::General => self.general(window, cx).into_any_element(),
            Tab::Extensions => self.extensions(cx).into_any_element(),
        };
        v_flex()
            .size_full()
            .track_focus(&self.focus)
            .text_sm()
            .text_color(Look::current(cx).ink)
            .child(chrome::title_bar(self, window, cx))
            .child(
                div()
                    .id("preferences-body")
                    .flex_1()
                    .min_h_0()
                    .child(div().px(px(16.)).pt(px(4.)).pb(px(20.)).child(body))
                    .overflow_y_scrollbar(),
            )
    }
}

#[cfg(test)]
mod tests;
