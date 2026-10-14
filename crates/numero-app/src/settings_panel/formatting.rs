//! Numero's own number format settings, which Numi leaves to the system:
//! the decimal and thousands separators, and the pixels per inch and per
//! em that CSS units convert with. Also the typed fields' handling,
//! precision's included.

use gpui::{App, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};

use super::Preferences;
use crate::settings::{GROUPINGS, MAX_PRECISION, Settings};
use crate::widgets::{chip, row};

/// `value` as a settings field shows it: no trailing `.0`.
pub(super) fn number(value: f64) -> String {
    let text = format!("{value}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

/// A typed precision, if it is a whole number Numi allows (0 to 10).
pub(crate) fn parse_precision(text: &str) -> Option<usize> {
    text.trim()
        .parse::<usize>()
        .ok()
        .filter(|precision| *precision <= MAX_PRECISION)
}

/// A typed ppi or em size, if it is a positive number.
pub(crate) fn parse_size(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.)
}

impl Preferences {
    pub(super) fn on_precision_event(
        &mut self,
        field: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            InputEvent::Change => {
                if let Some(precision) = parse_precision(&field.read(cx).value()) {
                    self.update(|settings| settings.precision = precision, cx);
                }
            }
            // Leaving the field shows what was taken.
            InputEvent::Blur => self.show_precision(window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Focus => {}
        }
    }

    /// Moves precision by `by` and shows it in its field.
    pub(super) fn step_precision(
        &mut self,
        by: isize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.update(
            |settings| {
                settings.precision = settings
                    .precision
                    .saturating_add_signed(by)
                    .min(MAX_PRECISION);
            },
            cx,
        );
        self.show_precision(window, cx);
    }

    fn show_precision(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let precision = self.settings(cx).precision.to_string();
        self.precision.update(cx, |field, cx| {
            if field.value() != precision {
                field.set_value(precision, window, cx);
            }
        });
    }

    pub(super) fn on_size_event(
        &mut self,
        field: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let Some(value) = parse_size(&field.read(cx).value()) else {
            return;
        };
        let ppi = field == &self.ppi;
        self.update(
            |settings| {
                if ppi {
                    settings.ppi = value;
                } else {
                    settings.em_px = value;
                }
            },
            cx,
        );
    }

    /// The number format section, below Numi's own settings.
    pub(super) fn formatting(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let (ppi, em) = (self.ppi.clone(), self.em.clone());
        let decimal = self.decimal_chips(settings, cx);
        let grouping = self.grouping_chips(settings, cx);
        v_flex()
            .gap_2()
            .child(heading("Number format", cx))
            .child(row("Decimal separator", decimal, cx))
            .child(row("Thousands separator", grouping, cx))
            .child(row(
                "Pixels per inch",
                Input::new(&ppi).xsmall().w(px(64.)),
                cx,
            ))
            .child(row(
                "Pixels per em",
                Input::new(&em).xsmall().w(px(64.)),
                cx,
            ))
    }

    fn decimal_chips(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let chips = ['.', ','].into_iter().enumerate().map(|(ix, separator)| {
            chip(
                ("decimal", ix),
                separator.to_string(),
                settings.decimal_separator == separator,
                cx.listener(move |this, _, _, cx| {
                    this.update(|settings| settings.decimal_separator = separator, cx);
                }),
                cx,
            )
        });
        h_flex().gap_0p5().children(chips.collect::<Vec<_>>())
    }

    fn grouping_chips(
        &self,
        settings: &Settings,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let chips = GROUPINGS.into_iter().enumerate().map(|(ix, grouping)| {
            let label = match grouping {
                None => "None".to_owned(),
                Some(' ') => "Space".to_owned(),
                Some(other) => other.to_string(),
            };
            chip(
                ("grouping", ix),
                label,
                settings.grouping_separator == grouping,
                cx.listener(move |this, _, _, cx| {
                    this.update(|settings| settings.grouping_separator = grouping, cx);
                }),
                cx,
            )
        });
        h_flex().gap_0p5().children(chips.collect::<Vec<_>>())
    }
}

/// A section's heading.
pub(super) fn heading(title: &'static str, cx: &App) -> impl IntoElement + use<> {
    div()
        .pt_3()
        .pb_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_takes_whole_numbers_up_to_ten() {
        assert_eq!(parse_precision(" 4 "), Some(4));
        assert_eq!(parse_precision("10"), Some(10));
        assert_eq!(parse_precision("11"), None);
        assert_eq!(parse_precision("2.5"), None);
        assert_eq!(parse_precision("-1"), None);
    }

    #[test]
    fn sizes_take_positive_numbers() {
        assert_eq!(parse_size("96"), Some(96.));
        assert_eq!(parse_size("0"), None);
        assert_eq!(parse_size("abc"), None);
        assert_eq!(number(16.), "16");
        assert_eq!(number(1.5), "1.5");
    }
}
