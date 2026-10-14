//! The calculator: the open calculation on the left, each line's result on
//! the right, level with it, and the grand total at the bottom.
//!
//! The whole calculation is evaluated again after every edit, on the UI
//! thread: the engine is fast and a calculation is small. The results then
//! colour the editor's text and fill the results column.
//!
//! - `calculations`: saving, and adding, opening and deleting calculations.
//! - `preferences`: the settings, applied live, and zooming.
//! - `exporting`: the Export menu's copies, "Save as…" and Import.
//! - `copying`: copying a result or the total, and what the footer says.
//! - `commands`: the Edit menu's commands on the calculation's text.
//! - `pane`: dragging the line between the text and the results.
//! - `main_window`: where menus and the tray find the calculator.
//! - `sync`: the calculations folder as other devices change it.

mod calculations;
mod commands;
mod copying;
mod exporting;
pub(crate) mod main_window;
mod pane;
mod preferences;
pub(crate) mod surround;
mod sync;

use std::path::PathBuf;

use chrono::Utc;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, FocusHandle, Focusable, Pixels,
    ScrollWheelEvent, SharedString, Subscription, Task, Window, px,
};
use gpui_component::input::{
    EditorState, InputEvent, InputState, TextDecoration, TextDecorationCollection,
};
use numero_engine::{Engine, EvalContext, LineResult, Rates};

pub(crate) use self::preferences::Zoom;
use crate::document::{self, ResultRow};
use crate::library::{CalculationId, Entry, FolderWatcher, Library, LibraryPaths, SAMPLE};
use crate::rates;
use crate::settings::{Settings, SettingsFile};
use crate::theme::Palette;

/// Where the calculator keeps things. `None` keeps nothing, as in tests.
#[derive(Debug, Default)]
pub(crate) struct Sources {
    /// Where calculations are kept; the settings may choose the folder.
    pub library: Option<LibraryPaths>,
    pub settings: Option<SettingsFile>,
    /// The folder JavaScript extensions load from.
    pub extensions: Option<PathBuf>,
}

impl Sources {
    /// Everything where the platform keeps an app's data and settings.
    pub(crate) fn from_env() -> Self {
        let library = LibraryPaths::from_env();
        if library.is_none() {
            tracing::warn!("no data directory; calculations won't be saved");
        }
        Self {
            library,
            settings: SettingsFile::from_env(),
            extensions: numero_extensions::extensions_dir(),
        }
    }
}

pub(crate) struct Calculator {
    pub(crate) editor: Entity<EditorState>,
    highlights: TextDecorationCollection,
    engine: Engine,
    pub(crate) results: Vec<LineResult>,
    pub(crate) rows: Vec<ResultRow>,
    /// The bottom bar's total, when the calculation has one.
    pub(crate) grand_total: Option<String>,
    pub(crate) palette: Palette,
    pub(crate) settings: Settings,
    settings_file: Option<SettingsFile>,
    /// Where extensions load from, and what loading them said.
    pub(crate) extensions_dir: Option<PathBuf>,
    pub(crate) extension_log: Vec<String>,
    /// The line whose result was just copied.
    pub(crate) copied: Option<usize>,
    /// What the footer says for a moment after a command, such as "Total
    /// copied".
    pub(crate) notice: Option<SharedString>,
    /// The line whose result is under the pointer.
    pub(crate) hovered_result: Option<usize>,
    /// Whether the pointer is on the total.
    pub(crate) hovered_total: bool,
    /// Whether a press on the title bar will move the window once the
    /// pointer moves.
    pub(crate) moving_window: bool,
    /// A drag of the line between the text and the results, under way.
    pub(crate) divider_drag: Option<pane::DividerDrag>,
    /// The window's width as last drawn, for the dynamic text size.
    viewport_width: Pixels,
    /// The calculator's own window.
    window: AnyWindowHandle,
    library: Option<Library>,
    library_paths: Option<LibraryPaths>,
    /// The folder used before the one chosen last, when the new one was
    /// empty: Preferences offers to move the calculations over.
    pub(crate) move_offer: Option<PathBuf>,
    _watcher: Option<FolderWatcher>,
    _watch_task: Option<Task<()>>,
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
    _notice_reset: Option<Task<()>>,
    _rates: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Calculator {
    /// Opens the calculation open last, or a new one with the sample.
    pub(crate) fn new(sources: Sources, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let Sources {
            library: library_paths,
            settings: settings_file,
            extensions: extensions_dir,
        } = sources;
        let settings = settings_file
            .as_ref()
            .map(SettingsFile::load)
            .unwrap_or_default();
        let mut library = library_paths
            .as_ref()
            .map(|paths| paths.open(settings.calculations_dir.as_deref()));
        if let Some(library) = &library {
            tracing::info!(folder = %library.folder().display(), "opening the calculations");
        }
        let (current, text) = open_last(library.as_mut());
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
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, Self::on_editor_event),
            cx.subscribe_in(&search, window, Self::on_search_event),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_appearance(window, cx);
            }),
            cx.on_app_quit(|this, cx| {
                this.save_now(cx);
                async {}
            }),
        ];
        let mut engine = Engine::new(settings.config());
        let extension_log = load_extensions(extensions_dir.as_deref(), &mut engine);
        main_window::register(window.window_handle(), cx.entity().downgrade(), cx);
        // A close the platform asks for (macOS's traffic light, the
        // compositor) saves first; the app's own close paths call
        // `main_window::save` themselves.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.save_now(cx));
            }
            true
        });
        let mut calculator = Self {
            editor,
            highlights,
            engine,
            results: Vec::new(),
            rows: Vec::new(),
            grand_total: None,
            palette: Palette::current(cx),
            settings,
            settings_file,
            extensions_dir,
            extension_log,
            copied: None,
            notice: None,
            hovered_result: None,
            hovered_total: false,
            moving_window: false,
            divider_drag: None,
            viewport_width: window.viewport_size().width,
            window: window.window_handle(),
            library,
            library_paths,
            move_offer: None,
            _watcher: None,
            _watch_task: None,
            current,
            list_open: false,
            search,
            entries: Vec::new(),
            pending_delete: None,
            revision: 0,
            saved_revision: 0,
            _save: None,
            _notice_reset: None,
            _rates: None,
            _subscriptions: subscriptions,
        };
        calculator.apply_appearance(window, cx);
        calculator.evaluate(cx);
        calculator.watch_library(cx);
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

    /// Evaluates the whole calculation and shows the results and the total.
    pub(crate) fn evaluate(&mut self, cx: &mut Context<'_, Self>) {
        let text = self.text(cx);
        let evaluation = self.engine.evaluate_document(&text, &EvalContext::system());
        self.results = evaluation.lines;
        self.grand_total = evaluation
            .grand_total
            .filter(|total| !total.trim().is_empty());
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

    /// Notes the window's width, which the dynamic text size follows.
    pub(crate) fn set_viewport_width(&mut self, width: Pixels) {
        self.viewport_width = width;
    }

    /// The text size of the calculation and its results.
    pub(crate) fn text_size(&self) -> Pixels {
        px(self.settings.points(self.viewport_width / px(1.)))
    }

    /// The height of one line, as the editor laid it out.
    pub(crate) fn line_height(&self, cx: &App) -> Pixels {
        self.editor
            .read(cx)
            .line_height()
            .unwrap_or(self.text_size() * 1.5)
    }
}

/// The calculation open last, or the welcome sample when nothing is kept.
fn open_last(library: Option<&mut Library>) -> (Option<CalculationId>, String) {
    let opened = library.and_then(|library| match library.open_last(SAMPLE) {
        Ok(opened) => Some(opened),
        Err(error) => {
            tracing::error!(%error, "couldn't open a calculation; edits won't be saved");
            None
        }
    });
    match opened {
        Some((id, text)) => (Some(id), text),
        None => (None, SAMPLE.to_owned()),
    }
}

/// Runs the extensions in `dir` into `engine`, once, before the first
/// evaluation (`docs/numi-spec.md` §12.8), and returns what they logged.
fn load_extensions(dir: Option<&std::path::Path>, engine: &mut Engine) -> Vec<String> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let log = numero_extensions::load(dir, engine);
    for line in &log.lines {
        tracing::info!(target: "numero::extensions", "{line}");
    }
    log.lines
}

impl Focusable for Calculator {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

#[cfg(test)]
mod tests;
