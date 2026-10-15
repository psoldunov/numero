//! The settings, applied as they change and saved, and zooming, which moves
//! the text size by a shortcut.
//!
//! The Preferences window changes the settings from its own window, so
//! nothing here needs the calculator's: what does (the theme) runs once
//! the change is over, in the calculator's window.

use gpui::{Context, Window};
use gpui_component::{Theme, ThemeMode};

use super::Calculator;
use crate::settings::{Appearance, MAX_ZOOM, MIN_ZOOM, Settings};

/// How far one zoom moves the text size.
const ZOOM_STEP: f32 = 2.;

/// A change to the text size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zoom {
    In,
    Out,
    Reset,
}

impl Zoom {
    /// The zoom after this one, from `zoom`.
    pub(crate) fn apply(self, zoom: f32) -> f32 {
        match self {
            Self::In => (zoom + ZOOM_STEP).min(MAX_ZOOM),
            Self::Out => (zoom - ZOOM_STEP).max(MIN_ZOOM),
            Self::Reset => 0.,
        }
    }
}

impl Calculator {
    pub(crate) fn zoom(&mut self, zoom: Zoom, cx: &mut Context<'_, Self>) {
        self.update_settings(|settings| settings.zoom = zoom.apply(settings.zoom), cx);
    }

    /// Changes the settings, keeps them in range, applies them and saves
    /// them, then tells the rest of the app.
    pub(crate) fn update_settings(
        &mut self,
        change: impl FnOnce(&mut Settings),
        cx: &mut Context<'_, Self>,
    ) {
        let mut next = self.settings.clone();
        change(&mut next);
        let next = next.checked();
        if next == self.settings {
            return;
        }
        let old = std::mem::replace(&mut self.settings, next);
        if let Some(file) = &self.settings_file
            && let Err(error) = file.save(&self.settings)
        {
            tracing::error!(%error, "couldn't save the settings");
        }
        self.engine.set_config(self.settings.config());
        if old.appearance != self.settings.appearance {
            self.in_window(cx, Self::apply_appearance);
        }
        let new = self.settings.clone();
        // After the change is over, so the system's side may reach the
        // window this change came from.
        cx.defer(move |cx| crate::system::settings_changed(&old, &new, cx));
        self.evaluate(cx);
    }

    /// Puts the theme in the chosen mode, or the desktop's.
    pub(super) fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.settings.appearance {
            Appearance::System => Theme::sync_system_appearance(Some(window), cx),
            Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
            Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
        }
        // The Preferences window draws with the theme too.
        cx.refresh_windows();
        self.follow_theme(cx);
    }
}
