//! What the window shows for a document, as plain data: where each line
//! starts, which ranges to colour, and which lines have a result.
//!
//! The engine answers per line, with spans relative to the line. The editor
//! wants byte ranges into the whole text, and the results column wants one
//! entry per line that has something to show. Nothing here touches GPUI.

use std::ops::Range;

use numero_engine::{LineKind, LineResult, TokenClass};

/// The byte offset at which each `\n`-separated line of `text` starts. As
/// many entries as the engine returns results: an empty text has one line.
pub(crate) fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
        .collect()
}

/// The spans of every line, moved to byte ranges into the whole `text`.
///
/// Spans that run past their line, are empty, or don't fall on character
/// boundaries are dropped: the engine and the editor may briefly disagree
/// on the text, and a bad range must never reach the editor.
pub(crate) fn highlight_ranges(
    text: &str,
    results: &[LineResult],
) -> Vec<(Range<usize>, TokenClass)> {
    let starts = line_starts(text);
    starts
        .iter()
        .zip(results)
        .flat_map(|(&start, result)| {
            let line = line_at(text, start);
            result.spans.iter().filter_map(move |span| {
                let range = &span.range;
                let valid = range.start < range.end
                    && range.end <= line.len()
                    && line.is_char_boundary(range.start)
                    && line.is_char_boundary(range.end);
                valid.then(|| (start + range.start..start + range.end, span.class))
            })
        })
        .collect()
}

/// The line of `text` that starts at byte `start`, without its `\n`.
fn line_at(text: &str, start: usize) -> &str {
    let rest = text.get(start..).unwrap_or_default();
    rest.split('\n').next().unwrap_or_default()
}

/// A line's entry in the results column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResultRow {
    /// The line's index in the document, from 0.
    pub line: usize,
    /// What the column shows and what a click copies.
    pub display: String,
}

/// The lines that have a value to show. Numi shows nothing for headers,
/// comments, empty lines and errors, and neither does Numero.
pub(crate) fn result_rows(results: &[LineResult]) -> Vec<ResultRow> {
    results
        .iter()
        .enumerate()
        .filter(|(_, result)| result.kind == LineKind::Value)
        .filter_map(|(line, result)| {
            let display = result.display.as_deref()?.trim();
            (!display.is_empty()).then(|| ResultRow {
                line,
                display: display.to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use numero_engine::{Config, Engine, Span};

    use super::*;

    fn value(display: &str, spans: Vec<Span>) -> LineResult {
        LineResult {
            kind: LineKind::Value,
            display: Some(display.to_owned()),
            number: None,
            spans,
        }
    }

    fn empty() -> LineResult {
        LineResult {
            kind: LineKind::Empty,
            display: None,
            number: None,
            spans: Vec::new(),
        }
    }

    fn span(range: Range<usize>, class: TokenClass) -> Span {
        Span { range, class }
    }

    #[test]
    fn every_line_has_a_start_including_a_trailing_empty_one() {
        assert_eq!(line_starts(""), [0]);
        assert_eq!(line_starts("1"), [0]);
        assert_eq!(line_starts("1\n22\n"), [0, 2, 5]);
        assert_eq!(line_starts("\n\n"), [0, 1, 2]);
    }

    #[test]
    fn as_many_line_starts_as_the_engine_has_results() {
        let engine = Engine::new(Config::default());
        for text in ["", "1", "# a\n2\n\nx", "1\n", "\n\n3"] {
            assert_eq!(
                line_starts(text).len(),
                engine.evaluate(text).len(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn spans_move_to_their_line_in_the_document() {
        let text = "# Sum\n12 + 3\n";
        let results = vec![
            LineResult {
                kind: LineKind::Header,
                spans: vec![span(0..5, TokenClass::Header)],
                ..empty()
            },
            value(
                "15",
                vec![
                    span(0..2, TokenClass::Number),
                    span(3..4, TokenClass::Operator),
                    span(5..6, TokenClass::Number),
                ],
            ),
            empty(),
        ];

        let ranges = highlight_ranges(text, &results);

        assert_eq!(
            ranges,
            [
                (0..5, TokenClass::Header),
                (6..8, TokenClass::Number),
                (9..10, TokenClass::Operator),
                (11..12, TokenClass::Number),
            ]
        );
        for (range, _) in &ranges {
            assert!(text.get(range.clone()).is_some());
        }
    }

    #[test]
    fn spans_past_their_line_or_inside_a_character_are_dropped() {
        let text = "€5\n1";
        let results = vec![
            value(
                "5",
                vec![
                    // Inside the three bytes of `€`.
                    span(1..2, TokenClass::Currency),
                    span(3..4, TokenClass::Number),
                    // Runs into the next line.
                    span(3..6, TokenClass::Number),
                    span(2..2, TokenClass::Number),
                ],
            ),
            value("1", vec![span(0..1, TokenClass::Number)]),
        ];

        assert_eq!(
            highlight_ranges(text, &results),
            [(3..4, TokenClass::Number), (5..6, TokenClass::Number)]
        );
    }

    #[test]
    fn only_values_with_something_to_show_get_a_row() {
        let results = vec![
            LineResult {
                kind: LineKind::Header,
                ..empty()
            },
            value("3.2", Vec::new()),
            empty(),
            LineResult {
                kind: LineKind::Error,
                display: Some("unknown unit".into()),
                ..empty()
            },
            value("  ", Vec::new()),
            value("$ 9.00 ", Vec::new()),
        ];

        assert_eq!(
            result_rows(&results),
            [
                ResultRow {
                    line: 1,
                    display: "3.2".into()
                },
                ResultRow {
                    line: 5,
                    display: "$ 9.00".into()
                },
            ]
        );
    }
}
