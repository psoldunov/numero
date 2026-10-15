//! The Edit menu's own commands (`docs/numi-spec.md` §12.3): Copy Result,
//! Copy All, Delete All and Surround with Parentheses. Each edit goes
//! through the editor as typing would, so it can be undone and the
//! calculation is evaluated and saved as after any edit.

use std::ops::Range;

use gpui::{App, ClipboardItem, Context, EntityInputHandler as _, Window};

use super::{Calculator, surround};
use crate::export;

impl Calculator {
    /// The line the caret is on.
    pub(crate) fn caret_line(&self, cx: &App) -> usize {
        let editor = self.editor.read(cx);
        let caret = editor.cursor();
        editor
            .text()
            .to_string()
            .get(..caret)
            .map_or(0, |before| before.matches('\n').count())
    }

    /// Copies the result of the caret's line, if it has one.
    pub(crate) fn copy_result_at_caret(&mut self, cx: &mut Context<'_, Self>) {
        let line = self.caret_line(cx);
        self.copy_result(line, cx);
    }

    /// Copies the selected lines, or all of them, each with its result.
    pub(crate) fn copy_all(&mut self, cx: &mut Context<'_, Self>) {
        let selection = self.editor.read(cx).selected_range();
        let text = export::copy_all(&self.text(cx), &self.results, selection);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_notice("Copied with results", cx);
    }

    /// Empties the open calculation, as one edit that can be undone.
    pub(crate) fn delete_all(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let text = self.text(cx);
        self.replace_range(&text, 0..text.len(), "", window, cx);
    }

    /// Wraps the selection, or the caret line's expression, in parentheses.
    pub(crate) fn surround_with_parentheses(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let text = self.text(cx);
        let selection = self.editor.read(cx).selected_range();
        let Some(range) = surround::target(&text, selection, &self.results) else {
            return;
        };
        let wrapped = surround::wrapped(&text, &range);
        self.replace_range(&text, range, &wrapped, window, cx);
    }

    /// Replaces the bytes `range` of `text`, the editor's text, with
    /// `replacement`.
    fn replace_range(
        &mut self,
        text: &str,
        range: Range<usize>,
        replacement: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let utf16 = utf16_offset(text, range.start)..utf16_offset(text, range.end);
        self.editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(Some(utf16), replacement, window, cx);
            editor.focus(window, cx);
        });
    }
}

/// Byte `offset` of `text` in UTF-16 code units, as the editor's input
/// handler counts.
fn utf16_offset(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(0, |before| before.encode_utf16().count())
}

#[cfg(test)]
mod tests {
    use super::utf16_offset;

    #[test]
    fn offsets_count_utf16_units() {
        assert_eq!(utf16_offset("€5 + 1", 3), 1);
        assert_eq!(utf16_offset("a", 1), 1);
    }
}
