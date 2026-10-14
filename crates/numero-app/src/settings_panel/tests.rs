//! The Preferences window: each change applies at once and is saved.

use gpui::{
    AppContext as _, Entity, EntityInputHandler as _, Keystroke, TestAppContext, VisualTestContext,
};

use super::Preferences;
use crate::calculator::{Calculator, Sources};
use crate::library::LibraryPaths;
use crate::settings::{FontSize, SettingsFile};

/// A calculator kept in `dir` and the Preferences for it.
fn preferences<'a>(
    dir: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (
    Entity<Calculator>,
    Entity<Preferences>,
    &'a mut VisualTestContext,
) {
    cx.update(gpui_component::init);
    let sources = Sources {
        library: Some(LibraryPaths::in_dir(dir)),
        settings: Some(SettingsFile::in_dir(dir)),
        extensions: None,
    };
    let (calculator, cx) = cx.add_window_view(|window, cx| Calculator::new(sources, window, cx));
    let weak = calculator.downgrade();
    let preferences = cx.update(|window, cx| cx.new(|cx| Preferences::new(weak, window, cx)));
    cx.run_until_parked();
    (calculator, preferences, cx)
}

fn type_into(
    field: &Entity<gpui_component::input::InputState>,
    text: &str,
    cx: &mut VisualTestContext,
) {
    cx.update(|window, cx| {
        field.update(cx, |field, cx| {
            field.select_all(window, cx);
            field.replace_text_in_range(None, text, window, cx);
        });
    });
    cx.run_until_parked();
}

fn saved(dir: &std::path::Path) -> crate::settings::Settings {
    SettingsFile::in_dir(dir).load()
}

#[gpui::test]
fn typed_precision_applies_when_numi_would_take_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, preferences, cx) = preferences(dir.path(), cx);
    let field = preferences.read_with(cx, |preferences, _| preferences.precision.clone());

    type_into(&field, "5", cx);
    assert_eq!(saved(dir.path()).precision, 5);

    type_into(&field, "42", cx);
    assert_eq!(saved(dir.path()).precision, 5);

    cx.update(|window, cx| {
        preferences.update(cx, |preferences, cx| {
            preferences.step_precision(1, window, cx)
        });
    });
    assert_eq!(saved(dir.path()).precision, 6);
    assert_eq!(
        field.read_with(cx, |field, _| field.value().to_string()),
        "6"
    );
}

#[gpui::test]
fn choices_apply_to_the_calculator_and_are_kept(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (calculator, preferences, cx) = preferences(dir.path(), cx);

    preferences.update(cx, |preferences, cx| {
        preferences.update(
            |settings| {
                settings.text_size = FontSize::Large;
                settings.always_on_top = true;
                settings.menu_bar_mode = true;
            },
            cx,
        );
    });
    cx.run_until_parked();

    let kept = saved(dir.path());
    assert_eq!(kept.text_size, FontSize::Large);
    assert!(kept.always_on_top && kept.menu_bar_mode);
    assert_eq!(
        calculator.read_with(cx, |calculator, _| calculator.settings.clone()),
        kept
    );
}

#[gpui::test]
fn the_recorder_takes_a_combination_and_escape_gives_up(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, preferences, cx) = preferences(dir.path(), cx);
    let press = |source: &str| Keystroke::parse(source).unwrap();

    cx.update(|window, cx| {
        preferences.update(cx, |preferences, cx| {
            preferences.start_recording(window, cx)
        });
    });
    let refused = preferences.update(cx, |preferences, cx| preferences.recorded(&press("k"), cx));
    assert!(refused);
    assert_eq!(saved(dir.path()).hotkey, None);

    preferences.update(cx, |preferences, cx| {
        preferences.recorded(&press("ctrl-alt-space"), cx)
    });
    assert_eq!(saved(dir.path()).hotkey.as_deref(), Some("ctrl-alt-space"));

    cx.update(|window, cx| {
        preferences.update(cx, |preferences, cx| {
            preferences.start_recording(window, cx)
        });
    });
    preferences.update(cx, |preferences, cx| {
        preferences.recorded(&press("escape"), cx)
    });
    assert!(!preferences.read_with(cx, |preferences, _| preferences.recorder.is_recording()));
    assert_eq!(saved(dir.path()).hotkey.as_deref(), Some("ctrl-alt-space"));
}

#[gpui::test]
fn the_ppi_field_takes_positive_numbers(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, preferences, cx) = preferences(dir.path(), cx);
    let field = preferences.read_with(cx, |preferences, _| preferences.ppi.clone());

    type_into(&field, "132", cx);
    assert_eq!(saved(dir.path()).ppi, 132.);
    type_into(&field, "-4", cx);
    assert_eq!(saved(dir.path()).ppi, 132.);
}
