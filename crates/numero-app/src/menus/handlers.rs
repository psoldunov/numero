//! What each command does. Every handler is app-wide, and acts once GPUI's
//! dispatch is over: a menu item or a tray click runs while GPUI holds the
//! window it came through (concierge's `docs/gpui.md`, "Menus").

use gpui::{Action, App, Window};

use super::samples::{SAMPLES, SYNTAX_GUIDE};
use super::{
    CloseWindow, CopyAll, CopyResult, CopyToClipboard, DeleteAll, ExportFile, HideApp, HideOthers,
    ImportFile, Minimize, NewCalculation, OpenDocs, OpenExtensionsFolder, OpenPreferences,
    OpenSample, ShowAll, ShowList, SurroundWithParentheses, ZoomWindow,
};
use crate::calculator::main_window::with_calculator;
use crate::settings_panel;

pub(super) fn register(cx: &mut App) {
    on_calculator::<NewCalculation>(cx, |calculator, window, cx| {
        calculator.set_list_open(false, window, cx);
        calculator.new_calculation(window, cx);
    });
    on_calculator::<ImportFile>(cx, |calculator, _, cx| calculator.import_file(cx));
    on_calculator::<ExportFile>(cx, |calculator, _, cx| calculator.save_as(cx));
    on_calculator::<CopyToClipboard>(cx, |calculator, _, cx| calculator.copy_with_results(cx));
    on_calculator::<ShowList>(cx, |calculator, window, cx| {
        calculator.set_list_open(true, window, cx);
    });
    on_calculator::<CopyResult>(cx, |calculator, _, cx| calculator.copy_result_at_caret(cx));
    on_calculator::<CopyAll>(cx, |calculator, _, cx| calculator.copy_all(cx));
    on_calculator::<DeleteAll>(cx, |calculator, window, cx| {
        calculator.delete_all(window, cx)
    });
    on_calculator::<SurroundWithParentheses>(cx, |calculator, window, cx| {
        calculator.surround_with_parentheses(window, cx);
    });
    on_calculator::<OpenDocs>(cx, |calculator, window, cx| {
        calculator.open_new_with(SYNTAX_GUIDE, window, cx);
    });
    on_calculator::<OpenExtensionsFolder>(cx, |calculator, _, cx| {
        calculator.open_extensions_folder(cx);
    });
    cx.on_action(|sample: &OpenSample, cx| {
        let Some(text) = SAMPLES.get(sample.0).map(|sample| sample.text) else {
            return;
        };
        with_calculator(cx, move |calculator, window, cx| {
            calculator.open_new_with(text, window, cx);
        });
    });
    on_app::<OpenPreferences>(cx, settings_panel::open);
    on_app::<HideApp>(cx, |cx| cx.hide());
    on_app::<HideOthers>(cx, |cx| cx.hide_other_apps());
    on_app::<ShowAll>(cx, |cx| cx.unhide_other_apps());
    cx.on_action(|_: &CloseWindow, cx| {
        cx.defer(|cx| {
            crate::calculator::main_window::save(cx);
            let Some(active) = cx.active_window() else {
                return;
            };
            if let Err(error) = active.update(cx, |_, window, _| window.remove_window()) {
                tracing::warn!(%error, "couldn't reach the window in front");
            }
        });
    });
    on_window::<Minimize>(cx, |window| window.minimize_window());
    on_window::<ZoomWindow>(cx, |window| window.zoom_window());
}

/// Runs `act` on the calculator when `A` is dispatched.
fn on_calculator<A: Action>(
    cx: &mut App,
    act: fn(
        &mut crate::calculator::Calculator,
        &mut Window,
        &mut gpui::Context<'_, crate::calculator::Calculator>,
    ),
) {
    cx.on_action(move |_: &A, cx| with_calculator(cx, act));
}

/// Runs `act` once the dispatch of `A` is over.
fn on_app<A: Action>(cx: &mut App, act: fn(&mut App)) {
    cx.on_action(move |_: &A, cx| cx.defer(act));
}

/// Runs `act` on the window in front when `A` is dispatched.
fn on_window<A: Action>(cx: &mut App, act: fn(&mut Window)) {
    cx.on_action(move |_: &A, cx| {
        cx.defer(move |cx| {
            let Some(active) = cx.active_window() else {
                return;
            };
            if let Err(error) = active.update(cx, |_, window, _| act(window)) {
                tracing::warn!(%error, "couldn't reach the window in front");
            }
        });
    });
}
