//! The calculator: the open calculation on the left, each line's result on
//! the right, level with it.
//!
//! The whole calculation is evaluated again after every edit, on the UI
//! thread: the engine is fast and a calculation is small. The results then
//! colour the editor's text and fill the results column.
//!
//! - `calculations`: saving, and adding, opening and deleting calculations.
//! - `preferences`: the settings, applied live, and zooming.
//! - `exporting`: the Export menu's copies and "Save as…".

mod calculations;
mod exporting;
mod preferences;

use std::time::Duration;

use chrono::Utc;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, Focusable, Pixels,
    ScrollWheelEvent, Subscription, Task, Window, px,
};
use gpui_component::input::{
    EditorState, InputEvent, InputState, TextDecoration, TextDecorationCollection,
};
use numero_engine::{Engine, LineResult, Rates};

pub(crate) use self::preferences::Zoom;
use crate::document::{self, ResultRow};
use crate::library::{CalculationId, Entry, Library, SAMPLE};
use crate::rates;
use crate::settings::{Settings, SettingsFile};
use crate::theme::Palette;

/// How long a copied result says so.
const COPIED_FOR: Duration = Duration::from_millis(1200);

/// Where the calculator keeps things. `None` keeps nothing, as in tests.
#[derive(Debug, Default)]
pub(crate) struct Sources {
    pub library: Option<Library>,
    pub settings: Option<SettingsFile>,
}

pub(crate) struct Calculator {
    pub(crate) editor: Entity<EditorState>,
    highlights: TextDecorationCollection,
    engine: Engine,
    pub(crate) results: Vec<LineResult>,
    pub(crate) rows: Vec<ResultRow>,
    pub(crate) palette: Palette,
    pub(crate) settings: Settings,
    settings_file: Option<SettingsFile>,
    /// The fields of the settings that take typed numbers.
    pub(crate) ppi_input: Entity<InputState>,
    pub(crate) em_input: Entity<InputState>,
    /// The line whose result was just copied.
    pub(crate) copied: Option<usize>,
    /// The line whose result is under the pointer.
    pub(crate) hovered_result: Option<usize>,
    /// Whether a press on the title bar will move the window once the
    /// pointer moves.
    pub(crate) moving_window: bool,
    library: Option<Library>,
    /// The open calculation; `None` when nothing is kept on disk.
    pub(crate) current: Option<CalculationId>,
    /// The list of calculations: whether it is open, its search field and
    /// the calculations it shows.
    pub(crate) list_open: bool,
    pub(crate) search: Entity<InputState>,
    pub(crate) entries: Vec<Entry>,
    /// The calculation whose delete was pressed once, waiting for the
    /// second press that confirms it.
    pub(crate) pending_delete: Option<CalculationId>,
    /// Counts edits, so a save knows whether it caught the latest one.
    revision: u64,
    saved_revision: u64,
    _save: Option<Task<()>>,
    _copied_reset: Option<Task<()>>,
    _rates: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Calculator {
    /// Opens the calculation open last, or a new one with the sample.
    pub(crate) fn new(sources: Sources, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let Sources {
            mut library,
            settings: settings_file,
        } = sources;
        let opened = library
            .as_mut()
            .and_then(|library| match library.open_last() {
                Ok(opened) => Some(opened),
                Err(error) => {
                    tracing::error!(%error, "couldn't open a calculation; edits won't be saved");
                    None
                }
            });
        let (current, text) = match opened {
            Some((id, text)) => (Some(id), text),
            None => (None, SAMPLE.to_owned()),
        };
        let settings = settings_file
            .as_ref()
            .map(SettingsFile::load)
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(false)
                .folding(false)
                .indent_guides(false)
                .soft_wrap(false)
                .auto_close(false)
                .smart_indent(false)
                .default_value(text)
        });
        let highlights = editor.update(cx, |editor, cx| {
            editor.create_decorations_collection(Vec::new(), cx)
        });
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let ppi_input =
            cx.new(|cx| InputState::new(window, cx).default_value(number(settings.ppi)));
        let em_input =
            cx.new(|cx| InputState::new(window, cx).default_value(number(settings.em_px)));
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, Self::on_editor_event),
            cx.subscribe_in(&search, window, Self::on_search_event),
            cx.subscribe_in(&ppi_input, window, Self::on_number_field_event),
            cx.subscribe_in(&em_input, window, Self::on_number_field_event),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_appearance(window, cx);
            }),
            cx.on_app_quit(|this, cx| {
                this.save_now(cx);
                async {}
            }),
        ];
        let mut calculator = Self {
            editor,
            highlights,
            engine: Engine::new(settings.config()),
            results: Vec::new(),
            rows: Vec::new(),
            palette: Palette::current(cx),
            settings,
            settings_file,
            ppi_input,
            em_input,
            copied: None,
            hovered_result: None,
            moving_window: false,
            library,
            current,
            list_open: false,
            search,
            entries: Vec::new(),
            pending_delete: None,
            revision: 0,
            saved_revision: 0,
            _save: None,
            _copied_reset: None,
            _rates: None,
            _subscriptions: subscriptions,
        };
        calculator.apply_appearance(window, cx);
        calculator.evaluate(cx);
        calculator
    }

    /// Uses the cached rates now and fetches fresh ones in the background
    /// when they are missing or old.
    pub(crate) fn start_rates(&mut self, cx: &mut Context<'_, Self>) {
        let cached = rates::cached();
        if rates::needs_refresh(cached.as_ref(), Utc::now()) {
            let fetch = rates::fetch_in_background(cx);
            self._rates = Some(cx.spawn(async move |this, cx| {
                if let Some(rates) = fetch.await {
                    let _ = this.update(cx, |this, cx| this.set_rates(rates, cx));
                }
            }));
        } else {
            tracing::info!("cached rates are fresh");
        }
        if let Some(rates) = cached {
            self.set_rates(rates, cx);
        }
    }

    pub(crate) fn set_rates(&mut self, rates: Rates, cx: &mut Context<'_, Self>) {
        self.engine.set_rates(rates);
        self.evaluate(cx);
    }

    /// The open calculation as it stands.
    pub(crate) fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text().to_string()
    }

    fn on_editor_event(
        &mut self,
        _: &Entity<EditorState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.revision += 1;
            self.evaluate(cx);
            self.schedule_save(cx);
        }
    }

    /// Evaluates the whole calculation and shows the results.
    pub(crate) fn evaluate(&mut self, cx: &mut Context<'_, Self>) {
        let text = self.text(cx);
        self.results = self.engine.evaluate(&text);
        self.rows = document::result_rows(&self.results);
        self.highlight(&text, cx);
        cx.notify();
    }

    fn highlight(&self, text: &str, cx: &mut App) {
        let decorations = document::highlight_ranges(text, &self.results)
            .into_iter()
            .map(|(range, class)| TextDecoration::new(range, self.palette.token_style(class)))
            .collect();
        self.highlights.set(decorations, cx);
    }

    /// Follows a change between light and dark.
    fn follow_theme(&mut self, cx: &mut Context<'_, Self>) {
        let palette = Palette::current(cx);
        if palette != self.palette {
            self.palette = palette;
            let text = self.text(cx);
            self.highlight(&text, cx);
            cx.notify();
        }
    }

    /// Copies the result of `line` and says so for a moment.
    pub(crate) fn copy_result(&mut self, line: usize, cx: &mut Context<'_, Self>) {
        let Some(row) = self.rows.iter().find(|row| row.line == line) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(row.display.clone()));
        self.copied = Some(line);
        self._copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = None;
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

    /// What the footer says: "Result copied" just after a copy, "Click to
    /// copy" over a result, else nothing (`docs/numi-spec-ui.md` §4.2).
    pub(crate) fn footer_hint(&self) -> Option<&'static str> {
        if self.copied.is_some() {
            Some("Result copied")
        } else if self.hovered_result.is_some() {
            Some("Click to copy")
        } else {
            None
        }
    }

    /// Scrolls the editor by a wheel turned over the results column.
    pub(crate) fn scroll_editor(&mut self, event: &ScrollWheelEvent, cx: &mut Context<'_, Self>) {
        let line_height = self.line_height(cx);
        let delta = event.delta.pixel_delta(line_height);
        self.editor.update(cx, |editor, cx| {
            let mut offset = editor.scroll_offset();
            offset.y += delta.y;
            editor.set_scroll_offset(offset, cx);
        });
    }

    /// The text size of the calculation and its results.
    pub(crate) fn text_size(&self) -> Pixels {
        px(self.settings.font_size)
    }

    /// The height of one line, as the editor laid it out.
    pub(crate) fn line_height(&self, cx: &App) -> Pixels {
        self.editor
            .read(cx)
            .line_height()
            .unwrap_or(self.text_size() * 1.5)
    }
}

/// `value` as a settings field shows it: no trailing `.0`.
fn number(value: f64) -> String {
    let text = format!("{value}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

impl Focusable for Calculator {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

#[cfg(test)]
mod tests;
