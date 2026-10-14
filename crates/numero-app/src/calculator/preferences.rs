//! The settings, applied as they change and saved, and zooming, which is
//! the text size setting moved by a shortcut.

use gpui::{Context, Entity, Window};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{Theme, ThemeMode};

use super::Calculator;
use crate::settings::{Appearance, DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE, Settings};

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
    pub(crate) fn apply(self, size: f32) -> f32 {
        match self {
            Self::In => (size + ZOOM_STEP).min(MAX_FONT_SIZE),
            Self::Out => (size - ZOOM_STEP).max(MIN_FONT_SIZE),
            Self::Reset => DEFAULT_FONT_SIZE,
        }
    }
}

impl Calculator {
    pub(crate) fn zoom(&mut self, zoom: Zoom, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.update_settings(
            |settings| settings.font_size = zoom.apply(settings.font_size),
            window,
            cx,
        );
    }

    /// Changes the settings, keeps them in range, applies them and saves
    /// them.
    pub(crate) fn update_settings(
        &mut self,
        change: impl FnOnce(&mut Settings),
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mut next = self.settings.clone();
        change(&mut next);
        let next = next.checked();
        if next == self.settings {
            return;
        }
        let appearance_changed = next.appearance != self.settings.appearance;
        self.settings = next;
        if let Some(file) = &self.settings_file
            && let Err(error) = file.save(&self.settings)
        {
            tracing::error!(%error, "couldn't save the settings");
        }
        self.engine.set_config(self.settings.config());
        if appearance_changed {
            self.apply_appearance(window, cx);
        }
        self.evaluate(cx);
    }

    /// Puts the theme in the chosen mode, or the desktop's.
    pub(super) fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.settings.appearance {
            Appearance::System => Theme::sync_system_appearance(Some(window), cx),
            Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
            Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
        }
        self.follow_theme(cx);
    }

    /// Takes a typed ppi or em size once it reads as a positive number.
    pub(super) fn on_number_field_event(
        &mut self,
        field: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let Ok(value) = field.read(cx).value().trim().parse::<f64>() else {
            return;
        };
        if !(value.is_finite() && value > 0.) {
            return;
        }
        let ppi = field == &self.ppi_input;
        self.update_settings(
            |settings| {
                if ppi {
                    settings.ppi = value;
                } else {
                    settings.em_px = value;
                }
            },
            window,
            cx,
        );
    }
}
