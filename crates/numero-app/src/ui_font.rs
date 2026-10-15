//! The UI's sans family: the desktop's own font, in the title bar, the
//! popovers, the footer and the Preferences window. The calculator's text
//! and results stay in the theme's mono family.
//!
//! macOS needs nothing: the theme's default `.SystemUIFont` is SF Pro there.
//! On Linux GPUI maps `.SystemUIFont` to a family most desktops don't ship
//! and draws its own fallback, so the UI wouldn't match the desktop. At start
//! the theme takes, in order, the first installed family of:
//!
//! 1. KDE's `font` in `kdeglobals` (read from disk, so the first frame
//!    already has it);
//! 2. fontconfig's `sans-serif`, resolved through GPUI's text system;
//!
//! and then the family the settings portal names in
//! `org.gnome.desktop.interface` `font-name` (GNOME's and KDE's portals both
//! answer it), once it answers.

use gpui::App;

/// Points the theme's UI family at the desktop's font.
#[cfg(target_os = "linux")]
pub(crate) fn apply(cx: &mut App) {
    linux::apply(cx);
}

/// Nothing to do: `.SystemUIFont` is already the system font.
#[cfg(not(target_os = "linux"))]
pub(crate) fn apply(_cx: &mut App) {}

/// The family in a Pango font description, as GNOME writes `font-name`:
/// `"Cantarell 11"`, `"SF Pro Display  10"`, `"Noto Sans Bold Italic 10"`,
/// `"Inter, Noto Sans 11px"`. The size and the style words at the end go;
/// of a family list, the first family stays.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn family_from_pango(description: &str) -> Option<String> {
    const STYLE_WORDS: [&str; 24] = [
        "normal",
        "roman",
        "regular",
        "oblique",
        "italic",
        "small-caps",
        "thin",
        "ultra-light",
        "extra-light",
        "light",
        "semi-light",
        "demi-light",
        "book",
        "medium",
        "semi-bold",
        "demi-bold",
        "bold",
        "ultra-bold",
        "extra-bold",
        "heavy",
        "black",
        "condensed",
        "expanded",
        "ultra-heavy",
    ];
    let mut words: Vec<&str> = description.split_whitespace().collect();
    if words
        .last()
        .is_some_and(|last| last.trim_end_matches("px").parse::<f32>().is_ok())
    {
        words.pop();
    }
    while words
        .last()
        .is_some_and(|last| STYLE_WORDS.contains(&last.to_ascii_lowercase().as_str()))
    {
        words.pop();
    }
    let joined = words.join(" ");
    let family = joined.split(',').next()?.trim();
    (!family.is_empty()).then(|| family.to_owned())
}

/// The family in a Qt font string, as KDE writes `font` in `kdeglobals`:
/// `"SF Pro Display,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1"`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn family_from_qt(value: &str) -> Option<String> {
    let family = value.split(',').next()?.trim();
    (!family.is_empty()).then(|| family.to_owned())
}

/// `font` from the `[General]` group of a `kdeglobals` file.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn kde_general_font(kdeglobals: &str) -> Option<String> {
    let mut in_general = false;
    for line in kdeglobals.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_general = line == "[General]";
        } else if in_general && let Some(value) = line.strip_prefix("font=") {
            return family_from_qt(value);
        }
    }
    None
}

/// The installed family that `family` names, compared without case, in the
/// spelling the text system knows it by.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn installed<'a>(family: &str, installed: &'a [String]) -> Option<&'a str> {
    installed
        .iter()
        .find(|name| name.eq_ignore_ascii_case(family))
        .map(String::as_str)
}

#[cfg(target_os = "linux")]
mod linux {
    use gpui::{App, AppContext as _, SharedString, font};
    use gpui_component::Theme;

    use super::{family_from_pango, installed, kde_general_font};

    const PORTAL_NAMESPACE: &str = "org.gnome.desktop.interface";
    const PORTAL_KEY: &str = "font-name";

    pub(super) fn apply(cx: &mut App) {
        let names = cx.text_system().all_font_names();
        let first = kde_font()
            .and_then(|family| installed(&family, &names).map(str::to_owned))
            .or_else(|| fontconfig_sans(cx, &names));
        if let Some(family) = first {
            set(&family, cx);
        }
        let read = cx.background_spawn(portal_font());
        cx.spawn(async move |cx| {
            let Some(family) = read.await else {
                return;
            };
            cx.update(|cx| {
                let names = cx.text_system().all_font_names();
                match installed(&family, &names) {
                    Some(family) => set(family, cx),
                    None => tracing::info!(%family, "the desktop's font isn't installed"),
                }
            });
        })
        .detach();
    }

    /// Names `family` on the theme, unless it is already there.
    fn set(family: &str, cx: &mut App) {
        if Theme::global(cx).font_family.as_ref() == family {
            return;
        }
        tracing::info!(%family, "UI font");
        let family = SharedString::from(family.to_owned());
        Theme::update(cx, |theme| theme.font_family = family);
    }

    fn kde_font() -> Option<String> {
        let path = dirs::config_dir()?.join("kdeglobals");
        let text = std::fs::read_to_string(path).ok()?;
        kde_general_font(&text)
    }

    /// The installed family fontconfig gives for `sans-serif`.
    fn fontconfig_sans(cx: &App, names: &[String]) -> Option<String> {
        let text_system = cx.text_system();
        let resolved = text_system
            .get_font_for_id(text_system.resolve_font(&font("sans-serif")))
            .map(|font| font.family.to_string())?;
        installed(&resolved, names).map(str::to_owned)
    }

    async fn portal_font() -> Option<String> {
        let settings = ashpd::desktop::settings::Settings::new().await.ok()?;
        let description = settings
            .read::<String>(PORTAL_NAMESPACE, PORTAL_KEY)
            .await
            .ok()?;
        family_from_pango(&description)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pango_description_gives_its_family() {
        assert_eq!(
            family_from_pango("Cantarell 11").as_deref(),
            Some("Cantarell")
        );
        assert_eq!(
            family_from_pango("SF Pro Display  10").as_deref(),
            Some("SF Pro Display")
        );
        assert_eq!(
            family_from_pango("Noto Sans Bold Italic 10").as_deref(),
            Some("Noto Sans")
        );
        assert_eq!(
            family_from_pango("Inter, Noto Sans 11px").as_deref(),
            Some("Inter")
        );
        assert_eq!(
            family_from_pango("Adwaita Sans").as_deref(),
            Some("Adwaita Sans")
        );
        assert_eq!(family_from_pango("  12 "), None);
        assert_eq!(family_from_pango(""), None);
    }

    #[test]
    fn a_qt_font_string_gives_its_family() {
        assert_eq!(
            family_from_qt("SF Pro Display,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1").as_deref(),
            Some("SF Pro Display")
        );
        assert_eq!(family_from_qt(",10"), None);
    }

    #[test]
    fn kdeglobals_gives_the_general_font_only() {
        let file = "[WM]\nfont=Wrong,9\n\n[General]\nColorScheme=Breeze\nfont=Noto Sans,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1\nmenuFont=Other,10\n";
        assert_eq!(kde_general_font(file).as_deref(), Some("Noto Sans"));
        assert_eq!(kde_general_font("[General]\nColorScheme=Breeze\n"), None);
    }

    #[test]
    fn an_installed_family_matches_without_case() {
        let names = vec!["SF Pro Display".to_owned(), "Noto Sans".to_owned()];
        assert_eq!(installed("sf pro display", &names), Some("SF Pro Display"));
        assert_eq!(installed("Cantarell", &names), None);
    }
}
