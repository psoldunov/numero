//! The user's settings: how results are formatted, the text size, light or
//! dark, the result column's width and the window's place in the desktop.
//! Kept in `<config dir>/numero/settings.json` and applied as they change.
//!
//! Every value read from the file is checked: a field that is missing or
//! out of range takes its default, so a hand-edited file can't break the
//! engine or the window.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use numero_engine::Config;
use serde::{Deserialize, Serialize};

const FILE: &str = "numero/settings.json";

/// The text never gets smaller or larger than this, zoomed or not.
pub(crate) const MIN_FONT_SIZE: f32 = 10.;
pub(crate) const MAX_FONT_SIZE: f32 = 36.;
/// How far zooming may move the text from its font size's.
pub(crate) const MIN_ZOOM: f32 = -4.;
pub(crate) const MAX_ZOOM: f32 = 24.;
pub(crate) const MAX_PRECISION: usize = 10;

/// Numi's result column is 120 px wide to begin with
/// (`docs/numi-spec-ui.md` §1.4).
pub(crate) const DEFAULT_RESULT_PANE_WIDTH: f32 = 120.;
const MIN_RESULT_PANE_WIDTH: f32 = 40.;
const MAX_RESULT_PANE_WIDTH: f32 = 2000.;

/// The characters a number may group its thousands with.
pub(crate) const GROUPINGS: [Option<char>; 4] = [None, Some(','), Some('.'), Some(' ')];

/// Light or dark. Numi starts in its night theme (`nightTheme` defaults to
/// true, `docs/numi-spec.md` §12.2), so Numero does too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Appearance {
    /// As the desktop is.
    System,
    Light,
    #[default]
    Dark,
}

/// Numi's font size choice (`docs/numi-spec-ui.md` §1.3): three fixed
/// sizes, or Dynamic, which grows with the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FontSize {
    #[default]
    Dynamic,
    Small,
    Medium,
    Large,
}

impl FontSize {
    pub(crate) const ALL: [Self; 4] = [Self::Dynamic, Self::Small, Self::Medium, Self::Large];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Dynamic => "Dynamic",
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    /// The text size, in points, in a window `window_width` wide.
    pub(crate) fn points(self, window_width: f32) -> f32 {
        match self {
            Self::Small => 13.,
            Self::Medium => 15.,
            Self::Large => 17.,
            Self::Dynamic if window_width > 800. => 17.,
            Self::Dynamic if window_width > 600. => 15.,
            Self::Dynamic if window_width > 400. => 14.,
            Self::Dynamic => 13.,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub precision: usize,
    pub decimal_separator: char,
    pub grouping_separator: Option<char>,
    pub ppi: f64,
    pub em_px: f64,
    pub text_size: FontSize,
    /// What the zoom shortcuts added to the font size's text size, in px.
    pub zoom: f32,
    pub appearance: Appearance,
    /// The result column's width before the window's bounds clamp it, under
    /// Numi's own key.
    #[serde(rename = "resultPaneWidth")]
    pub result_pane_width: f32,
    /// Numi's "Show in menu bar": a tray / status item, no dock icon on
    /// macOS, and the window toggles from the item.
    pub menu_bar_mode: bool,
    /// Keep the window above others.
    pub always_on_top: bool,
    /// The global shortcut that shows and hides the window, as a GPUI
    /// keystroke (`ctrl-alt-space`). Numi ships none.
    pub hotkey: Option<String>,
    /// Where the calculations are kept, one `.txt` file each; `None` is the
    /// default folder under the data directory.
    pub calculations_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        let config = Config::default();
        Self {
            precision: config.precision,
            decimal_separator: config.decimal_separator,
            grouping_separator: config.grouping_separator,
            ppi: config.ppi,
            em_px: config.em_px,
            text_size: FontSize::default(),
            zoom: 0.,
            appearance: Appearance::default(),
            result_pane_width: DEFAULT_RESULT_PANE_WIDTH,
            menu_bar_mode: false,
            always_on_top: false,
            hotkey: None,
            calculations_dir: None,
        }
    }
}

impl Settings {
    /// The settings with every value in range: anything out of it takes its
    /// default, and a grouping separator equal to the decimal one is
    /// dropped, since `1.234` would read two ways.
    pub(crate) fn checked(self) -> Self {
        let default = Self::default();
        let positive = |value: f64, fallback: f64| {
            if value.is_finite() && value > 0. && value < 10_000. {
                value
            } else {
                fallback
            }
        };
        let decimal_separator = if matches!(self.decimal_separator, '.' | ',') {
            self.decimal_separator
        } else {
            default.decimal_separator
        };
        let grouping_separator = self
            .grouping_separator
            .filter(|grouping| GROUPINGS.contains(&Some(*grouping)))
            .filter(|grouping| *grouping != decimal_separator);
        Self {
            precision: self.precision.min(MAX_PRECISION),
            decimal_separator,
            grouping_separator,
            ppi: positive(self.ppi, default.ppi),
            em_px: positive(self.em_px, default.em_px),
            text_size: self.text_size,
            zoom: clamp_or(self.zoom, MIN_ZOOM, MAX_ZOOM, default.zoom),
            appearance: self.appearance,
            result_pane_width: clamp_or(
                self.result_pane_width,
                MIN_RESULT_PANE_WIDTH,
                MAX_RESULT_PANE_WIDTH,
                default.result_pane_width,
            ),
            menu_bar_mode: self.menu_bar_mode,
            always_on_top: self.always_on_top,
            hotkey: self.hotkey.filter(|hotkey| !hotkey.trim().is_empty()),
            calculations_dir: self.calculations_dir.filter(|dir| dir.is_absolute()),
        }
    }

    /// The engine's part of the settings.
    pub(crate) fn config(&self) -> Config {
        Config {
            precision: self.precision,
            decimal_separator: self.decimal_separator,
            grouping_separator: self.grouping_separator,
            ppi: self.ppi,
            em_px: self.em_px,
        }
    }

    /// The text size in a window `window_width` wide: the font size's,
    /// zoomed, within the limits.
    pub(crate) fn points(&self, window_width: f32) -> f32 {
        (self.text_size.points(window_width) + self.zoom).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
    }
}

/// `value` within `min..=max`, or `fallback` when it isn't a number.
fn clamp_or(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// Where the settings are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub(crate) fn from_env() -> Option<Self> {
        dirs::config_dir().map(|dir| Self::in_dir(&dir))
    }

    pub(crate) fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE),
        }
    }

    /// The saved settings, or the defaults when there are none or they
    /// can't be read.
    pub(crate) fn load(&self) -> Settings {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(error) => {
                tracing::error!(%error, path = %self.path.display(), "couldn't read the settings");
                return Settings::default();
            }
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => settings.checked(),
            Err(error) => {
                tracing::error!(%error, path = %self.path.display(), "the settings are unreadable; using the defaults");
                Settings::default()
            }
        }
    }

    pub(crate) fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests;
