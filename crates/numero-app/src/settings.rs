//! The user's settings: how results are formatted, the text size and light
//! or dark. Kept in `<config dir>/numero/settings.json` and applied as they
//! change.
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

/// Numi's text size in its default window: its Dynamic mode gives 13 up
/// to 400 px wide (`docs/numi-spec-ui.md` §1.3).
pub(crate) const DEFAULT_FONT_SIZE: f32 = 13.;
pub(crate) const MIN_FONT_SIZE: f32 = 10.;
pub(crate) const MAX_FONT_SIZE: f32 = 36.;
pub(crate) const MAX_PRECISION: usize = 10;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub precision: usize,
    pub decimal_separator: char,
    pub grouping_separator: Option<char>,
    pub ppi: f64,
    pub em_px: f64,
    pub font_size: f32,
    pub appearance: Appearance,
    /// Numi's "Show in menu bar": a tray / status item, no dock icon on
    /// macOS, and the window toggles from the item.
    pub menu_bar_mode: bool,
    /// Keep the window above others.
    pub always_on_top: bool,
    /// The global shortcut that shows and hides the window, as a GPUI
    /// keystroke (`ctrl-alt-space`). Numi ships none.
    pub hotkey: Option<String>,
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
            font_size: DEFAULT_FONT_SIZE,
            appearance: Appearance::default(),
            menu_bar_mode: false,
            always_on_top: false,
            hotkey: None,
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
            font_size: if self.font_size.is_finite() {
                self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
            } else {
                DEFAULT_FONT_SIZE
            },
            appearance: self.appearance,
            menu_bar_mode: self.menu_bar_mode,
            always_on_top: self.always_on_top,
            hotkey: self.hotkey.filter(|hotkey| !hotkey.trim().is_empty()),
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
mod tests {
    use super::*;

    #[test]
    fn saved_settings_load_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = SettingsFile::in_dir(dir.path());
        let settings = Settings {
            precision: 4,
            decimal_separator: ',',
            grouping_separator: Some('.'),
            appearance: Appearance::Dark,
            font_size: 20.,
            ..Settings::default()
        };

        file.save(&settings).unwrap();

        assert_eq!(file.load(), settings);
    }

    #[test]
    fn missing_or_broken_settings_are_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = SettingsFile::in_dir(dir.path());
        assert_eq!(file.load(), Settings::default());

        fs::create_dir_all(dir.path().join("numero")).unwrap();
        fs::write(dir.path().join(FILE), "{ nope").unwrap();
        assert_eq!(file.load(), Settings::default());
    }

    #[test]
    fn a_partial_file_keeps_the_rest_at_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = SettingsFile::in_dir(dir.path());
        fs::create_dir_all(dir.path().join("numero")).unwrap();
        fs::write(dir.path().join(FILE), r#"{"precision": 5}"#).unwrap();

        let settings = file.load();

        assert_eq!(settings.precision, 5);
        assert_eq!(settings.ppi, Settings::default().ppi);
    }

    #[test]
    fn values_out_of_range_take_their_defaults() {
        let settings = Settings {
            precision: 99,
            decimal_separator: 'x',
            grouping_separator: Some('.'),
            ppi: -3.,
            em_px: f64::NAN,
            font_size: 300.,
            appearance: Appearance::Light,
            hotkey: Some("  ".into()),
            ..Settings::default()
        }
        .checked();

        assert_eq!(settings.precision, MAX_PRECISION);
        assert_eq!(settings.decimal_separator, '.');
        // Equal to the decimal separator, so dropped.
        assert_eq!(settings.grouping_separator, None);
        assert_eq!(settings.ppi, Settings::default().ppi);
        assert_eq!(settings.em_px, Settings::default().em_px);
        assert_eq!(settings.font_size, MAX_FONT_SIZE);
        assert_eq!(settings.hotkey, None);
    }

    #[test]
    fn the_engine_gets_the_formatting_settings() {
        let settings = Settings {
            precision: 3,
            ..Settings::default()
        };
        assert_eq!(settings.config().precision, 3);
    }
}
