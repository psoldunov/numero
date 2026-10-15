use super::*;

#[test]
fn saved_settings_load_back() {
    let dir = tempfile::tempdir().unwrap();
    let file = SettingsFile::in_dir(dir.path());
    let settings = Settings {
        precision: 4,
        decimal_separator: ',',
        grouping_separator: Some('.'),
        appearance: Appearance::Light,
        text_size: FontSize::Large,
        zoom: 4.,
        result_pane_width: 180.,
        menu_bar_mode: true,
        always_on_top: true,
        hotkey: Some("ctrl-alt-space".into()),
        calculations_dir: Some(dir.path().join("synced")),
        ..Settings::default()
    };

    file.save(&settings).unwrap();

    assert_eq!(file.load(), settings);
}

#[test]
fn the_result_pane_width_is_kept_under_numis_key() {
    let dir = tempfile::tempdir().unwrap();
    let file = SettingsFile::in_dir(dir.path());
    let settings = Settings {
        result_pane_width: 150.,
        ..Settings::default()
    };

    file.save(&settings).unwrap();

    let json = fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert!(json.contains("\"resultPaneWidth\": 150.0"), "{json}");
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
    assert_eq!(settings.text_size, FontSize::Dynamic);
    assert_eq!(settings.result_pane_width, DEFAULT_RESULT_PANE_WIDTH);
}

#[test]
fn numis_defaults_hold() {
    let settings = Settings::default();
    assert_eq!(settings.precision, 2);
    assert_eq!(settings.text_size, FontSize::Dynamic);
    assert_eq!(settings.appearance, Appearance::Dark);
    assert_eq!(settings.result_pane_width, 120.);
    assert!(!settings.menu_bar_mode && !settings.always_on_top);
    assert_eq!(settings.hotkey, None);
}

#[test]
fn values_out_of_range_take_their_defaults() {
    let settings = Settings {
        precision: 99,
        decimal_separator: 'x',
        grouping_separator: Some('.'),
        ppi: -3.,
        em_px: f64::NAN,
        zoom: 300.,
        result_pane_width: f32::INFINITY,
        appearance: Appearance::Light,
        hotkey: Some("  ".into()),
        calculations_dir: Some("relative/folder".into()),
        ..Settings::default()
    }
    .checked();

    assert_eq!(settings.precision, MAX_PRECISION);
    assert_eq!(settings.decimal_separator, '.');
    // Equal to the decimal separator, so dropped.
    assert_eq!(settings.grouping_separator, None);
    assert_eq!(settings.ppi, Settings::default().ppi);
    assert_eq!(settings.em_px, Settings::default().em_px);
    assert_eq!(settings.zoom, MAX_ZOOM);
    assert_eq!(settings.result_pane_width, DEFAULT_RESULT_PANE_WIDTH);
    assert_eq!(settings.hotkey, None);
    assert_eq!(settings.calculations_dir, None);
}

#[test]
fn the_engine_gets_the_formatting_settings() {
    let settings = Settings {
        precision: 3,
        ..Settings::default()
    };
    assert_eq!(settings.config().precision, 3);
}

#[test]
fn dynamic_text_grows_with_the_window_as_numis_does() {
    let sizes: Vec<f32> = [370., 400., 401., 601., 801.]
        .into_iter()
        .map(|width| FontSize::Dynamic.points(width))
        .collect();
    assert_eq!(sizes, [13., 13., 14., 15., 17.]);
    assert_eq!(FontSize::Small.points(900.), 13.);
    assert_eq!(FontSize::Medium.points(300.), 15.);
    assert_eq!(FontSize::Large.points(300.), 17.);
}

#[test]
fn zoom_moves_the_text_size_within_its_limits() {
    let zoomed = |text_size, zoom| {
        Settings {
            text_size,
            zoom,
            ..Settings::default()
        }
        .points(370.)
    };
    assert_eq!(zoomed(FontSize::Small, 4.), 17.);
    assert_eq!(zoomed(FontSize::Small, -4.), MIN_FONT_SIZE);
    assert_eq!(zoomed(FontSize::Large, MAX_ZOOM), MAX_FONT_SIZE);
}
