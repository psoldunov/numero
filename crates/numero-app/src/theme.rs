//! Numero's colours: Numi's own light and night themes
//! (`docs/numi-spec-ui.md` §3), following the desktop's light or dark.
//!
//! The engine's token classes map onto Numi's tags: currencies draw as
//! units, operator symbols as plain text, and operator words (`in`, `of`)
//! as Numi's "operation". gpui-component's theme still supplies the caret
//! and the selection; the window's background, translucent where the
//! desktop blurs, comes from here.

use gpui::{App, HighlightStyle, Hsla, rgb};
use gpui_component::ActiveTheme as _;
use numero_engine::TokenClass;

/// How opaque the background is over the blurred desktop: enough that
/// text reads the same whatever is behind the window.
const BACKGROUND_ALPHA_LIGHT: f32 = 0.86;
const BACKGROUND_ALPHA_NIGHT: f32 = 0.88;

/// The colours a frame is drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    /// Translucent: the desktop shows through, blurred, where it can.
    /// `frame::see_through` makes it opaque where nothing blurs.
    pub background: Hsla,
    /// The window's title.
    pub title: Hsla,
    /// Quiet chrome text: the footer's hint, empty rows.
    pub muted: Hsla,
    /// The line between the text and the results.
    pub separator: Hsla,
    pub result: Hsla,
    /// A result under the pointer: its text and the pill behind it.
    pub result_hover_text: Hsla,
    pub result_hover: Hsla,
    /// The pill while the result is pressed.
    pub result_pressed: Hsla,
    /// The mark beside the open calculation in the list.
    pub selected_mark: Hsla,
    tokens: TokenColors,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TokenColors {
    text: Hsla,
    header: Hsla,
    label: Hsla,
    unit: Hsla,
    comment: Hsla,
    variable: Hsla,
    operation: Hsla,
    function: Hsla,
}

impl Palette {
    /// The palette for the theme's current mode.
    pub(crate) fn current(cx: &App) -> Self {
        if cx.theme().is_dark() {
            Self::night()
        } else {
            Self::light()
        }
    }

    pub(crate) fn light() -> Self {
        Self {
            background: with_alpha(color(0xffffff), BACKGROUND_ALPHA_LIGHT),
            title: color(0x111111),
            muted: color(0xcccccc),
            separator: color(0xdddddd),
            result: color(0x5ec930),
            result_hover_text: color(0xffffff),
            result_hover: color(0x30ca41),
            result_pressed: color(0x2ab439),
            selected_mark: color(0x06be24),
            tokens: TokenColors {
                text: color(0x677c8d),
                header: color(0x146788),
                label: color(0x146788),
                unit: color(0x84a9c5),
                comment: color(0xdbdbe2),
                variable: color(0x33a9e4),
                operation: color(0x33a9e4),
                function: color(0x33a9e4),
            },
        }
    }

    /// Numi's default look.
    pub(crate) fn night() -> Self {
        Self {
            background: with_alpha(color(0x212226), BACKGROUND_ALPHA_NIGHT),
            title: color(0x5e5e61),
            muted: color(0x555551),
            separator: color(0x3d3d3d),
            result: color(0x8dd32d),
            result_hover_text: color(0x27282d),
            result_hover: color(0x8dd32d),
            result_pressed: color(0x75b220),
            selected_mark: color(0x06be24),
            tokens: TokenColors {
                text: color(0xe3e7e9),
                header: color(0xffc22d),
                label: color(0xffc22d),
                unit: color(0xbdd4d2),
                comment: color(0x62626b),
                variable: color(0x6cc4ee),
                operation: color(0x6cc4ee),
                function: color(0x6cc4ee),
            },
        }
    }

    /// How text of `class` is drawn in the editor.
    pub(crate) fn token_style(&self, class: TokenClass) -> HighlightStyle {
        let tokens = &self.tokens;
        let color = match class {
            TokenClass::Number | TokenClass::Operator | TokenClass::Text => tokens.text,
            TokenClass::Unit | TokenClass::Currency => tokens.unit,
            TokenClass::Keyword => tokens.operation,
            TokenClass::Function => tokens.function,
            TokenClass::Variable => tokens.variable,
            TokenClass::Label => tokens.label,
            TokenClass::Comment => tokens.comment,
            TokenClass::Header => tokens.header,
        };
        HighlightStyle {
            color: Some(color),
            ..HighlightStyle::default()
        }
    }
}

fn color(hex: u32) -> Hsla {
    Hsla::from(rgb(hex))
}

fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_symbols_draw_as_text_and_currencies_as_units() {
        let night = Palette::night();
        let text = night.token_style(TokenClass::Text).color;
        assert_eq!(night.token_style(TokenClass::Number).color, text);
        assert_eq!(night.token_style(TokenClass::Operator).color, text);
        assert_eq!(
            night.token_style(TokenClass::Currency).color,
            night.token_style(TokenClass::Unit).color
        );
    }

    #[test]
    fn night_headers_are_numi_yellow() {
        let header = Palette::night().token_style(TokenClass::Header).color;
        assert_eq!(header, Some(color(0xffc22d)));
    }

    #[test]
    fn light_and_night_differ() {
        assert_ne!(Palette::light(), Palette::night());
    }
}
