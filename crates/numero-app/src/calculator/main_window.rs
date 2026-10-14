//! Where the menus, the shortcuts, the tray and the Preferences window find
//! the calculator: its window and its view, kept as a global when it is
//! built.
//!
//! A menu item or a tray click runs while GPUI holds the window it came
//! through, so [`with_calculator`] acts once that dispatch is over
//! (concierge's `docs/gpui.md`, "Menus").

use gpui::{AnyWindowHandle, App, Context, Entity, Global, WeakEntity, Window};

use super::Calculator;

struct MainCalculator {
    window: AnyWindowHandle,
    view: WeakEntity<Calculator>,
}

impl Global for MainCalculator {}

/// Notes the calculator built in `window`.
pub(super) fn register(window: AnyWindowHandle, view: WeakEntity<Calculator>, cx: &mut App) {
    cx.set_global(MainCalculator { window, view });
}

/// The calculator, while its window is open.
pub(crate) fn calculator(cx: &App) -> Option<Entity<Calculator>> {
    cx.try_global::<MainCalculator>()?.view.upgrade()
}

/// Saves the open calculation now, if the calculator exists: before any
/// close of its window, so the last edits inside the autosave delay are
/// kept.
pub(crate) fn save(cx: &mut App) {
    if let Some(calculator) = calculator(cx) {
        calculator.update(cx, |calculator, cx| calculator.save_now(cx));
    }
}

/// The calculator's window, while it is open.
pub(crate) fn window(cx: &App) -> Option<AnyWindowHandle> {
    cx.try_global::<MainCalculator>()
        .filter(|main| main.view.upgrade().is_some())
        .map(|main| main.window)
}

/// Runs `act` on the calculator in its window, once the current dispatch is
/// over.
pub(crate) fn with_calculator(
    cx: &mut App,
    act: impl FnOnce(&mut Calculator, &mut Window, &mut Context<'_, Calculator>) + 'static,
) {
    cx.defer(move |cx| {
        let (Some(window), Some(view)) = (window(cx), calculator(cx)) else {
            tracing::warn!("no calculator window to act on");
            return;
        };
        let acted = window.update(cx, |_, window, cx| {
            view.update(cx, |calculator, cx| act(calculator, window, cx));
        });
        if let Err(error) = acted {
            tracing::warn!(%error, "couldn't reach the calculator's window");
        }
    });
}

impl Calculator {
    /// Runs `act` on the calculator in its own window once the current
    /// update is over: a change may come from the Preferences window, or
    /// from inside the calculator's window, which is busy until then.
    pub(crate) fn in_window(
        &self,
        cx: &mut Context<'_, Self>,
        act: impl FnOnce(&mut Self, &mut Window, &mut Context<'_, Self>) + 'static,
    ) {
        let handle = self.window;
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let acted = handle.update(cx, |_, window, cx| {
                this.update(cx, |this, cx| act(this, window, cx))
            });
            if acted.is_err() {
                tracing::warn!("the calculator's window is gone");
            }
        });
    }
}
