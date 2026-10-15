//! The Preferences window's colours. Unlike the calculator's window it is
//! opaque: a form reads best on a solid ground, and the calculator's
//! translucent one left gpui-component's borders and fills, made for the
//! theme's own ground, all but invisible.
//!
//! Surfaces are told apart by a step in tone, never by a line: sections sit
//! on panels a step off the ground, and the choices and typed fields in
//! them on tracks a step back. The calculator's result green is the one
//! accent, for what is switched on and the one button that moves files.

use gpui::{App, Hsla, rgb};
use gpui_component::ActiveTheme as _;

use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    /// The window, behind everything.
    pub ground: Hsla,
    /// The panel a section's rows sit on.
    pub panel: Hsla,
    /// Inside a panel: behind a set of choices and a typed field.
    pub track: Hsla,
    /// The choice that is picked, on its track.
    pub selected: Hsla,
    pub button: Hsla,
    pub button_hover: Hsla,
    /// Labels, values and every other text that is read.
    pub ink: Hsla,
    /// Hints, and the choices that aren't picked.
    pub muted: Hsla,
    /// What is on, and the field being typed in.
    pub accent: Hsla,
    /// Text on the accent.
    pub accent_ink: Hsla,
    pub danger: Hsla,
}

impl Look {
    /// The look for the theme's current mode.
    pub(crate) fn current(cx: &App) -> Self {
        let danger = cx.theme().danger;
        if cx.theme().is_dark() {
            Self::night(danger)
        } else {
            Self::light(danger)
        }
    }

    /// The window's ground, for the frame to fill it with.
    pub(crate) fn ground(cx: &App) -> Hsla {
        Self::current(cx).ground
    }

    /// Numi's night background, made opaque, with lighter panels on it.
    pub(crate) fn night(danger: Hsla) -> Self {
        Self {
            ground: color(0x212226),
            panel: color(0x2c2d32),
            track: color(0x1e1f22),
            selected: color(0x47484e),
            button: color(0x3b3c42),
            button_hover: color(0x47484e),
            ink: color(0xe3e7e9),
            muted: color(0xa0a3aa),
            accent: Palette::night().result,
            accent_ink: color(0x1e1f22),
            danger,
        }
    }

    /// A light grey ground with white panels, as the calculator is white.
    pub(crate) fn light(danger: Hsla) -> Self {
        Self {
            ground: color(0xefeff2),
            panel: color(0xffffff),
            track: color(0xebebef),
            selected: color(0xffffff),
            button: color(0xebebef),
            button_hover: color(0xdfdfe4),
            ink: color(0x1d1d1f),
            muted: color(0x66666d),
            accent: Palette::light().result,
            accent_ink: color(0x1d1d1f),
            danger,
        }
    }
}

fn color(hex: u32) -> Hsla {
    Hsla::from(rgb(hex))
}

#[cfg(test)]
mod tests {
    use gpui::{Rgba, hsla};

    use super::*;

    /// WCAG's contrast ratio between two opaque colours.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let luminance = |color: Hsla| {
            let Rgba { r, g, b, .. } = color.to_rgb();
            let linear = |c: f32| {
                if c <= 0.039_28 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn looks() -> [Look; 2] {
        let danger = hsla(0., 0.8, 0.5, 1.);
        [Look::night(danger), Look::light(danger)]
    }

    #[test]
    fn text_reads_on_every_surface_it_sits_on() {
        for look in looks() {
            for ground in [
                look.ground,
                look.panel,
                look.track,
                look.selected,
                look.button,
            ] {
                assert!(contrast(look.ink, ground) >= 7., "{look:?}");
            }
            for ground in [look.ground, look.panel, look.track] {
                assert!(contrast(look.muted, ground) >= 4.5, "{look:?}");
            }
            assert!(contrast(look.accent_ink, look.accent) >= 4.5, "{look:?}");
        }
    }

    #[test]
    fn surfaces_step_apart() {
        for look in looks() {
            assert_ne!(look.ground, look.panel);
            assert_ne!(look.panel, look.track);
            assert_ne!(look.track, look.selected);
            assert_ne!(look.panel, look.button);
            assert!(look.ground.a >= 1., "the window is opaque");
        }
    }
}
