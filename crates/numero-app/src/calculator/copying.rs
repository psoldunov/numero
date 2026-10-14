//! Copying a line's result or the grand total, and what the footer says
//! meanwhile: "Click to copy" over a result, "Result copied" after a copy
//! (`docs/numi-spec-ui.md` §4.2 and §4.3).

use std::time::Duration;

use gpui::{ClipboardItem, Context, SharedString};

use super::Calculator;

/// How long a copy or a command says so in the footer.
const NOTICE_FOR: Duration = Duration::from_millis(1200);

impl Calculator {
    /// Copies the result of `line` and says so for a moment.
    pub(crate) fn copy_result(&mut self, line: usize, cx: &mut Context<'_, Self>) {
        let Some(row) = self.rows.iter().find(|row| row.line == line) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(row.display.clone()));
        self.copied = Some(line);
        self.show_notice("Result copied", cx);
    }

    /// Copies the grand total, as a click on it does.
    pub(crate) fn copy_total(&mut self, cx: &mut Context<'_, Self>) {
        let Some(total) = self.grand_total.clone() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(total));
        self.show_notice("Total copied", cx);
    }

    /// Shows `notice` in the footer for a moment.
    pub(crate) fn show_notice(
        &mut self,
        notice: impl Into<SharedString>,
        cx: &mut Context<'_, Self>,
    ) {
        self.notice = Some(notice.into());
        self._notice_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
                this.notice = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Follows the pointer onto and off a result, for the footer's hint.
    pub(crate) fn hover_result(&mut self, line: usize, hovered: bool, cx: &mut Context<'_, Self>) {
        if hovered {
            self.hovered_result = Some(line);
        } else if self.hovered_result == Some(line) {
            self.hovered_result = None;
        }
        cx.notify();
    }

    /// Follows the pointer onto and off the total.
    pub(crate) fn hover_total(&mut self, hovered: bool, cx: &mut Context<'_, Self>) {
        self.hovered_total = hovered;
        cx.notify();
    }

    /// What the footer says: a notice just after a copy or a command,
    /// "Click to copy" over a result or the total, else nothing.
    pub(crate) fn footer_hint(&self) -> Option<SharedString> {
        if let Some(notice) = &self.notice {
            Some(notice.clone())
        } else if self.hovered_result.is_some() || self.hovered_total {
            Some("Click to copy".into())
        } else {
            None
        }
    }
}
