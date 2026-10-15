//! A calculation as plain text, for the Export menu: the lines with their
//! results beside them, or the results alone.

use std::ops::Range;
use std::path::Path;

use numero_engine::{LineKind, LineResult};

/// How far the results stand from the longest line.
const GAP: usize = 4;

/// The display of each line that has a value, by line.
fn displays(results: &[LineResult]) -> impl Iterator<Item = Option<&str>> {
    results.iter().map(|result| {
        result
            .display
            .as_deref()
            .filter(|_| result.kind == LineKind::Value)
            .map(str::trim)
            .filter(|display| !display.is_empty())
    })
}

/// Every line, and after each one with a value, that value, the values
/// lined up in one column.
pub(crate) fn with_results(text: &str, results: &[LineResult]) -> String {
    let lines: Vec<&str> = text.split('\n').map(str::trim_end).collect();
    let width = lines
        .iter()
        .zip(displays(results))
        .filter(|(_, display)| display.is_some())
        .map(|(line, _)| line.chars().count())
        .max()
        .unwrap_or(0);
    lines
        .iter()
        .zip(displays(results).chain(std::iter::repeat(None)))
        .map(|(line, display)| match display {
            Some(display) => {
                let pad = width + GAP - line.chars().count();
                format!("{line}{}{display}", " ".repeat(pad))
            }
            None => (*line).to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What Edit → Copy All copies: the lines `selection` touches (every line
/// when it is empty), each followed by ` = ` and its result where it has
/// one, a line apart.
pub(crate) fn copy_all(text: &str, results: &[LineResult], selection: Range<usize>) -> String {
    let (first, last) = if selection.start < selection.end {
        let end = selection.end.min(text.len());
        // A selection ending at a line's start doesn't take that line.
        let last_byte = end.saturating_sub(1).max(selection.start);
        (line_of(text, selection.start), line_of(text, last_byte))
    } else {
        (0, usize::MAX)
    };
    text.split('\n')
        .zip(displays(results).chain(std::iter::repeat(None)))
        .enumerate()
        .filter(|(index, _)| (first..=last).contains(index))
        .map(|(_, (line, display))| match display {
            Some(display) => format!("{} = {display}", line.trim_end()),
            None => line.trim_end().to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The index of the line holding byte `offset` of `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(0, |before| before.matches('\n').count())
}

/// The values alone, one per line, in order.
pub(crate) fn results_only(results: &[LineResult]) -> String {
    displays(results).flatten().collect::<Vec<_>>().join("\n")
}

/// What "Save as…" writes to `path`: the calculation as it is for a
/// `.numi` file, and with its results for anything else.
pub(crate) fn for_path(path: &Path, text: &str, results: &[LineResult]) -> String {
    let raw = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("numi"));
    if raw {
        text.to_owned()
    } else {
        with_results(text, results)
    }
}

/// The longest name "Save as…" suggests, before its extension.
const NAME_LENGTH: usize = 40;

/// The file name "Save as…" suggests for a calculation titled `title`:
/// its letters, digits, spaces and dashes, or "Calculation".
pub(crate) fn file_name(title: Option<&str>) -> String {
    let name: String = title
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .take(NAME_LENGTH)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "Calculation.txt".to_owned()
    } else {
        format!("{name}.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: LineKind, display: Option<&str>) -> LineResult {
        LineResult {
            kind,
            display: display.map(str::to_owned),
            number: None,
            spans: Vec::new(),
        }
    }

    fn sample() -> (&'static str, Vec<LineResult>) {
        (
            "# Trip\n120 EUR in USD\n\n3 * 4",
            vec![
                line(LineKind::Header, None),
                line(LineKind::Value, Some("$ 130.20")),
                line(LineKind::Empty, None),
                line(LineKind::Value, Some("12")),
            ],
        )
    }

    #[test]
    fn results_line_up_after_the_longest_line_with_one() {
        let (text, results) = sample();
        assert_eq!(
            with_results(text, &results),
            "# Trip\n120 EUR in USD    $ 130.20\n\n3 * 4             12"
        );
    }

    #[test]
    fn copy_all_puts_each_result_after_an_equals_sign() {
        let (text, results) = sample();
        assert_eq!(
            copy_all(text, &results, 0..0),
            "# Trip\n120 EUR in USD = $ 130.20\n\n3 * 4 = 12"
        );
    }

    #[test]
    fn copy_all_takes_only_the_selected_lines() {
        let (text, results) = sample();
        // From inside line 1 to the start of line 3: lines 1 and 2.
        let start = text.find("EUR").unwrap();
        let end = text.find("3 *").unwrap();
        assert_eq!(
            copy_all(text, &results, start..end),
            "120 EUR in USD = $ 130.20\n"
        );
    }

    #[test]
    fn results_only_lists_the_values() {
        let (_, results) = sample();
        assert_eq!(results_only(&results), "$ 130.20\n12");
    }

    #[test]
    fn errors_export_without_a_result() {
        let results = vec![line(LineKind::Error, Some("unknown unit"))];
        assert_eq!(with_results("5 florps", &results), "5 florps");
        assert_eq!(results_only(&results), "");
    }

    #[test]
    fn a_numi_file_gets_the_calculation_as_typed() {
        let (text, results) = sample();
        assert_eq!(for_path(Path::new("trip.numi"), text, &results), text);
        assert!(for_path(Path::new("trip.txt"), text, &results).contains("$ 130.20"));
    }

    #[test]
    fn the_suggested_name_comes_from_the_title() {
        assert_eq!(file_name(Some("# Trip to Rome")), "Trip to Rome.txt");
        assert_eq!(file_name(Some("../../x")), "x.txt");
        assert_eq!(file_name(Some("8 / 2")), "8  2.txt");
        assert_eq!(file_name(None), "Calculation.txt");
        assert_eq!(file_name(Some("/// *")), "Calculation.txt");
    }
}
