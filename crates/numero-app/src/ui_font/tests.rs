use super::*;

fn font(family: &str, size_px: Option<f32>) -> Option<DesktopFont> {
    Some(DesktopFont {
        family: family.to_owned(),
        size_px,
    })
}

#[test]
fn a_pango_description_gives_its_family_and_size() {
    assert_eq!(parse_pango("Cantarell 12"), font("Cantarell", Some(16.)));
    assert_eq!(
        parse_pango("SF Pro Display  10"),
        font("SF Pro Display", Some(10. * PX_PER_PT))
    );
    assert_eq!(
        parse_pango("Noto Sans Bold Italic 9"),
        font("Noto Sans", Some(12.))
    );
    assert_eq!(
        parse_pango("Inter, Noto Sans 14px"),
        font("Inter", Some(14.))
    );
    assert_eq!(parse_pango("Adwaita Sans"), font("Adwaita Sans", None));
    assert_eq!(
        parse_pango("JetBrainsMonoNL Nerd Font  10"),
        font("JetBrainsMonoNL Nerd Font", Some(10. * PX_PER_PT))
    );
    assert_eq!(parse_pango("  12 "), None);
    assert_eq!(parse_pango(""), None);
}

#[test]
fn a_qt_font_string_gives_its_family_and_size() {
    assert_eq!(
        parse_qt("SF Pro Display,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1"),
        font("SF Pro Display", Some(10. * PX_PER_PT))
    );
    assert_eq!(
        parse_qt("Noto Sans,-1,15,5,400"),
        font("Noto Sans", Some(15.))
    );
    assert_eq!(parse_qt("Noto Sans"), font("Noto Sans", None));
    assert_eq!(parse_qt(",10"), None);
}

#[test]
fn kdeglobals_gives_the_general_group_font_for_each_role() {
    let file = "[WM]\nfont=Wrong,9\n\n[General]\nColorScheme=Breeze\n\
                fixed=Hack,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,,0,0\n\
                font=Noto Sans,9,-1,5,400,0,0,0,0,0,0,0,0,0,0,1\nmenuFont=Other,10\n";
    assert_eq!(
        kde_general_font(file, Role::Ui.kde_key()),
        font("Noto Sans", Some(12.))
    );
    assert_eq!(
        kde_general_font(file, Role::Mono.kde_key()),
        font("Hack", Some(10. * PX_PER_PT))
    );
    assert_eq!(
        kde_general_font("[General]\nColorScheme=Breeze\n", "font"),
        None
    );
}

#[test]
fn each_role_reads_its_own_settings() {
    assert_eq!(
        (
            Role::Mono.kde_key(),
            Role::Mono.portal_key(),
            Role::Mono.generic_family()
        ),
        ("fixed", "monospace-font-name", "monospace")
    );
    assert_eq!(
        (
            Role::Ui.kde_key(),
            Role::Ui.portal_key(),
            Role::Ui.generic_family()
        ),
        ("font", "font-name", "sans-serif")
    );
}

#[test]
fn labels_come_out_at_the_desktop_size() {
    let ten_points = DesktopFont {
        family: "SF Pro Display".to_owned(),
        size_px: Some(10. * PX_PER_PT),
    };
    let rem = ten_points.rem_px().unwrap();
    assert!((rem * LABEL_REMS - 10. * PX_PER_PT).abs() < 0.001);
    // No size: the theme keeps its own.
    let no_size = DesktopFont {
        size_px: None,
        ..ten_points.clone()
    };
    assert_eq!(no_size.rem_px(), None);
    // A broken size is clamped.
    let huge = DesktopFont {
        size_px: Some(400.),
        ..ten_points
    };
    assert_eq!(huge.rem_px(), Some(TEXT_PX.1 / LABEL_REMS));
}

#[test]
fn an_installed_family_matches_without_case() {
    let names = vec!["SF Pro Display".to_owned(), "Noto Sans".to_owned()];
    assert_eq!(installed("sf pro display", &names), Some("SF Pro Display"));
    assert_eq!(installed("Cantarell", &names), None);
}
