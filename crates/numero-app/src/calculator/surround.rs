//! What Edit → Surround with Parentheses wraps: the selection, or else the
//! expression on the caret's line, without its label (`Price:`), the name
//! it is assigned to (`x =`) or a trailing comment.

use std::ops::Range;

use numero_engine::{LineResult, Span, TokenClass};

/// The byte range of `text` to wrap, given the editor's `selection` (an
/// empty one is the caret) and the line results; `None` when the caret's
/// line holds no expression.
pub(crate) fn target(
    text: &str,
    selection: Range<usize>,
    results: &[LineResult],
) -> Option<Range<usize>> {
    if selection.start < selection.end {
        return Some(selection);
    }
    let caret = selection.start.min(text.len());
    let line_index = text[..caret].matches('\n').count();
    let start = text[..caret].rfind('\n').map_or(0, |newline| newline + 1);
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |len| start + len);
    let spans = results
        .get(line_index)
        .map_or(&[][..], |result| result.spans.as_slice());
    expression(&text[start..end], spans).map(|range| start + range.start..start + range.end)
}

/// `text` with `range` wrapped in parentheses.
pub(crate) fn wrapped(text: &str, range: &Range<usize>) -> String {
    format!("({})", &text[range.clone()])
}

/// The expression's range within one `line`, whose highlight `spans` the
/// engine gave.
fn expression(line: &str, spans: &[Span]) -> Option<Range<usize>> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with("//") {
        return None;
    }
    let start = after_prefix(line, spans);
    let end = spans
        .iter()
        .find(|span| span.class == TokenClass::Comment && span.range.start >= start)
        .map_or(line.len(), |comment| comment.range.start);
    trim(line, start..end)
}

/// Where the expression starts: past a leading label and an assignment.
fn after_prefix(line: &str, spans: &[Span]) -> usize {
    let mut rest = spans.iter().peekable();
    let mut start = 0;
    while let Some(label) = rest.next_if(|span| span.class == TokenClass::Label) {
        start = label.range.end;
    }
    let assignment = rest
        .next_if(|span| span.class == TokenClass::Variable)
        .zip(rest.peek())
        .filter(|(_, operator)| line.get(operator.range.clone()).map(str::trim) == Some("="));
    if let Some((_, operator)) = assignment {
        start = operator.range.end;
    }
    // A label's colon may sit outside its span.
    let skipped = line[start..]
        .find(|c: char| !(c.is_whitespace() || c == ':'))
        .unwrap_or(line.len() - start);
    if start > 0 { start + skipped } else { start }
}

/// `range` of `line` without the whitespace at either end; `None` if
/// nothing is left.
fn trim(line: &str, range: Range<usize>) -> Option<Range<usize>> {
    let slice = line.get(range.clone())?;
    let leading = slice.len() - slice.trim_start().len();
    let trailing = slice.len() - slice.trim_end().len();
    let trimmed = range.start + leading..range.end - trailing;
    (trimmed.start < trimmed.end).then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use numero_engine::{Config, Engine};

    use super::*;

    fn wrap_at(text: &str, caret: usize) -> Option<String> {
        let results = Engine::new(Config::default()).evaluate(text);
        let range = target(text, caret..caret, &results)?;
        let mut out = text.to_owned();
        out.replace_range(range.clone(), &wrapped(text, &range));
        Some(out)
    }

    #[test]
    fn a_selection_is_wrapped_as_it_is() {
        let text = "2 + 3 * 4";
        assert_eq!(target(text, 4..9, &[]), Some(4..9));
        assert_eq!(wrapped(text, &(4..9)), "(3 * 4)");
    }

    #[test]
    fn the_caret_line_is_wrapped_without_its_indent() {
        assert_eq!(
            wrap_at("1\n  2 + 3  \n4", 4).as_deref(),
            Some("1\n  (2 + 3)  \n4")
        );
    }

    #[test]
    fn a_label_and_an_assignment_stay_outside() {
        assert_eq!(
            wrap_at("Price: $6 times 5", 3).as_deref(),
            Some("Price: ($6 times 5)")
        );
        assert_eq!(wrap_at("x = 4 + 5", 0).as_deref(), Some("x = (4 + 5)"));
    }

    #[test]
    fn headers_comments_and_empty_lines_have_nothing_to_wrap() {
        assert_eq!(wrap_at("# Trip", 2), None);
        assert_eq!(wrap_at("// note", 2), None);
        assert_eq!(wrap_at("1\n\n2", 2), None);
    }
}
