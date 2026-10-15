//! How results are written: Numi's precision, and Numero's own decimal and
//! thousands separators, which Numi leaves to the system. Also the pixels
//! per inch and per em that CSS units convert with, and the typed fields'
//! handling, precision's included.

use gpui::{Context, Entity, IntoElement, ParentElement as _, Styled as _, Window, px};
use gpui_component::h_flex;
use gpui_component::input::{InputEvent, InputState};

use super::Preferences;
use super::controls::{
    Choice, OnClick, choices, hint, number_field, panel, row, section, step_button,
};
use super::look::Look;
use crate::settings::{GROUPINGS, MAX_PRECISION, Settings};

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
            InputEvent::Blur => {
                self.show_precision(window, cx);
                cx.notify();
            }
            // The field's ring follows the focus.
            InputEvent::Focus => cx.notify(),
            InputEvent::PressEnter { .. } => {}
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
        match event {
            InputEvent::Change => {}
            // The field's ring follows the focus.
            InputEvent::Focus | InputEvent::Blur => {
                cx.notify();
                return;
            }
            InputEvent::PressEnter { .. } => return,
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

    /// How results are written: their decimal places and separators.
    pub(super) fn results_section(
        &self,
        settings: &Settings,
        look: &Look,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let precision = self.precision_stepper(settings, look, window, cx);
        let decimal = ['.', ',']
            .into_iter()
            .map(|separator| Choice {
                label: decimal_sample(separator).into(),
                selected: settings.decimal_separator == separator,
                on_click: Box::new(cx.listener(move |this, _, _, cx| {
                    this.update(|settings| settings.decimal_separator = separator, cx);
                })),
            })
            .collect();
        let grouping = groupings_beside(settings.decimal_separator)
            .map(|grouping| Choice {
                label: grouping_sample(grouping).into(),
                selected: settings.grouping_separator == grouping,
                on_click: Box::new(cx.listener(move |this, _, _, cx| {
                    this.update(|settings| settings.grouping_separator = grouping, cx);
                })),
            })
            .collect();
        section(
            "Results",
            panel(look)
                .child(row(
                    "Precision",
                    Some(hint("Decimal places shown", look.muted)),
                    precision,
                    look,
                ))
                .child(row(
                    "Decimal separator",
                    None,
                    choices("decimal", decimal, look),
                    look,
                ))
                .child(row(
                    "Thousands separator",
                    None,
                    choices("grouping", grouping, look),
                    look,
                )),
            look,
        )
    }

    /// `−`, the typed precision, `+`; a step past 0 or 10 can't be taken.
    fn precision_stepper(
        &self,
        settings: &Settings,
        look: &Look,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let down = (settings.precision > 0).then(|| -> OnClick {
            Box::new(cx.listener(|this, _, window, cx| this.step_precision(-1, window, cx)))
        });
        let up = (settings.precision < MAX_PRECISION).then(|| -> OnClick {
            Box::new(cx.listener(|this, _, window, cx| this.step_precision(1, window, cx)))
        });
        h_flex()
            .gap_1()
            .child(step_button("precision-down", "−", down, look))
            .child(number_field(&self.precision, px(44.), look, window, cx))
            .child(step_button("precision-up", "+", up, look))
    }

    /// The sizes CSS units convert with.
    pub(super) fn units_section(
        &self,
        look: &Look,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        section(
            "CSS units",
            panel(look)
                .child(row(
                    "Pixels per inch",
                    Some(hint("How px converts to cm and inches", look.muted)),
                    number_field(&self.ppi, px(64.), look, window, cx),
                    look,
                ))
                .child(row(
                    "Pixels per em",
                    Some(hint("How em converts to px", look.muted)),
                    number_field(&self.em, px(64.), look, window, cx),
                    look,
                )),
            look,
        )
    }
}

/// A number written with `separator` before its decimals, as the choice
/// shows it.
fn decimal_sample(separator: char) -> String {
    format!("1{separator}5")
}

/// A thousand grouped with `grouping`, as the choice shows it.
fn grouping_sample(grouping: Option<char>) -> String {
    grouping.map_or_else(|| "1000".to_owned(), |grouping| format!("1{grouping}000"))
}

/// The thousands separators that can stand beside `decimal`: every one but
/// itself, which `Settings::checked` would drop.
fn groupings_beside(decimal: char) -> impl Iterator<Item = Option<char>> {
    GROUPINGS
        .into_iter()
        .filter(move |grouping| *grouping != Some(decimal))
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

    #[test]
    fn the_choices_show_what_a_number_will_look_like() {
        assert_eq!(decimal_sample(','), "1,5");
        assert_eq!(grouping_sample(None), "1000");
        assert_eq!(grouping_sample(Some(' ')), "1 000");
        assert_eq!(grouping_sample(Some('.')), "1.000");
    }

    #[test]
    fn the_decimal_separator_is_never_offered_for_thousands() {
        let beside_point: Vec<_> = groupings_beside('.').collect();
        assert_eq!(beside_point, [None, Some(','), Some(' ')]);
        let beside_comma: Vec<_> = groupings_beside(',').collect();
        assert_eq!(beside_comma, [None, Some('.'), Some(' ')]);
    }
}
