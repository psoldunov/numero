//! The settings, in a popover over the gear in the bottom-left corner. Each
//! change applies at once and is saved.

use gpui::{App, Div, Entity, IntoElement, ParentElement as _, Styled as _, div, px};
use gpui_component::input::Input;
use gpui_component::{Sizable as _, h_flex, v_flex};

use crate::calculator::Calculator;
use crate::settings::{Appearance, GROUPINGS, MAX_PRECISION, Settings};
use crate::widgets::{chip, row};

const WIDTH: f32 = 290.;

/// The panel for `this`'s settings.
pub(crate) fn content(this: &Entity<Calculator>, cx: &mut App) -> Div {
    let calculator = this.read(cx);
    let settings = calculator.settings.clone();
    let (ppi, em) = (calculator.ppi_input.clone(), calculator.em_input.clone());
    v_flex()
        .w(px(WIDTH))
        .gap_2()
        .text_sm()
        .child(row(
            "Decimal places",
            stepper(
                this,
                "precision",
                settings.precision.to_string(),
                |settings, by| {
                    settings.precision = settings
                        .precision
                        .saturating_add_signed(by)
                        .min(MAX_PRECISION);
                },
                cx,
            ),
            cx,
        ))
        .child(row(
            "Decimal separator",
            decimal_chips(this, &settings, cx),
            cx,
        ))
        .child(row("Thousands", grouping_chips(this, &settings, cx), cx))
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
        .child(row(
            "Text size",
            stepper(
                this,
                "font-size",
                format!("{}", settings.font_size),
                |settings, by| {
                    settings.font_size += by as f32;
                },
                cx,
            ),
            cx,
        ))
        .child(row("Appearance", appearance_chips(this, &settings, cx), cx))
}

/// Applies `change` to `this`'s settings.
fn update(
    this: &Entity<Calculator>,
    change: impl FnOnce(&mut Settings) + 'static,
    window: &mut gpui::Window,
    cx: &mut App,
) {
    this.update(cx, |calculator, cx| {
        calculator.update_settings(change, window, cx)
    });
}

/// `−`, the value, `+`.
fn stepper(
    this: &Entity<Calculator>,
    id: &'static str,
    value: String,
    step: fn(&mut Settings, isize),
    cx: &App,
) -> impl IntoElement {
    let (down, up) = (this.clone(), this.clone());
    h_flex()
        .gap_1()
        .child(chip(
            (id, 0usize),
            "−",
            false,
            move |_, window, cx| {
                update(&down, move |settings| step(settings, -1), window, cx);
            },
            cx,
        ))
        .child(div().min_w(px(24.)).flex().justify_center().child(value))
        .child(chip(
            (id, 1usize),
            "+",
            false,
            move |_, window, cx| {
                update(&up, move |settings| step(settings, 1), window, cx);
            },
            cx,
        ))
}

fn decimal_chips(this: &Entity<Calculator>, settings: &Settings, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_0p5()
        .children(['.', ','].into_iter().enumerate().map(|(ix, separator)| {
            let this = this.clone();
            chip(
                ("decimal", ix),
                separator.to_string(),
                settings.decimal_separator == separator,
                move |_, window, cx| {
                    update(
                        &this,
                        move |settings| settings.decimal_separator = separator,
                        window,
                        cx,
                    );
                },
                cx,
            )
        }))
}

fn grouping_chips(this: &Entity<Calculator>, settings: &Settings, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_0p5()
        .children(GROUPINGS.into_iter().enumerate().map(|(ix, grouping)| {
            let this = this.clone();
            let label = match grouping {
                None => "None".to_owned(),
                Some(' ') => "Space".to_owned(),
                Some(other) => other.to_string(),
            };
            let selected = settings.grouping_separator == grouping;
            chip(
                ("grouping", ix),
                label,
                selected,
                move |_, window, cx| {
                    update(
                        &this,
                        move |settings| settings.grouping_separator = grouping,
                        window,
                        cx,
                    );
                },
                cx,
            )
        }))
}

fn appearance_chips(this: &Entity<Calculator>, settings: &Settings, cx: &App) -> impl IntoElement {
    let choices = [
        (Appearance::System, "System"),
        (Appearance::Light, "Light"),
        (Appearance::Dark, "Dark"),
    ];
    h_flex().gap_0p5().children(
        choices
            .into_iter()
            .enumerate()
            .map(|(ix, (appearance, label))| {
                let this = this.clone();
                chip(
                    ("appearance", ix),
                    label,
                    settings.appearance == appearance,
                    move |_, window, cx| {
                        update(
                            &this,
                            move |settings| settings.appearance = appearance,
                            window,
                            cx,
                        );
                    },
                    cx,
                )
            }),
    )
}
