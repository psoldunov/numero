//! The desktop's fonts in the app: its sans family and size for the title
//! bar, the popovers, the footer and the Preferences window, and its
//! monospace family for the calculator's text and results, which keep Numi's
//! text sizes.
//!
//! macOS needs nothing: the theme's default `.SystemUIFont` is SF Pro there,
//! at gpui-component's sizes, which follow AppKit's.
//!
//! On Linux GPUI maps `.SystemUIFont` to a family most desktops don't ship
//! and draws its own fallback, and gpui-component's mono is a bundled pick,
//! so neither would match the desktop. At start each [`Role`] takes the
//! first of:
//!
//! 1. KDE's setting in `kdeglobals` (`font`, `fixed`), read from disk, so
//!    the first frame already has it;
//! 2. fontconfig's generic family (`sans-serif`, `monospace`), resolved
//!    through GPUI's text system;
//!
//! and then the font the settings portal names in
//! `org.gnome.desktop.interface` (`font-name`, `monospace-font-name`; GNOME's
//! and KDE's portals both answer it), once it answers. A family that isn't
//! installed is skipped.
//!
//! The UI size goes on the theme's rem, which gpui-component's text and
//! controls are measured in: labels are `text_sm`, 0.875 rem, so the rem is
//! the desktop's text size over 0.875, and buttons and fields scale with it
//! as they do in Qt and GTK.

use gpui::App;

/// Pixels per point at the 96 dpi Linux desktops lay text out at; display
/// scaling comes on top, in GPUI's logical pixels.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const PX_PER_PT: f32 = 96. / 72.;

/// `text_sm`, gpui-component's label size, in rems.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LABEL_REMS: f32 = 0.875;

/// The desktop text sizes, in pixels, the UI follows; anything outside is
/// clamped, so a broken setting can't make the UI unusable.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const TEXT_PX: (f32, f32) = (10., 24.);

/// Points the theme's fonts at the desktop's.
#[cfg(target_os = "linux")]
pub(crate) fn apply(cx: &mut App) {
    linux::apply(cx);
}

/// Nothing to do: `.SystemUIFont` is already the system font.
#[cfg(not(target_os = "linux"))]
pub(crate) fn apply(_cx: &mut App) {}

/// Which of the desktop's fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
enum Role {
    /// The sans UI font: the theme's family and rem.
    Ui,
    /// The fixed-width font: the theme's mono family; sizes stay Numi's.
    Mono,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl Role {
    const ALL: [Self; 2] = [Self::Ui, Self::Mono];

    /// Its key in the `[General]` group of `kdeglobals`.
    fn kde_key(self) -> &'static str {
        match self {
            Self::Ui => "font",
            Self::Mono => "fixed",
        }
    }

    /// Its key in the portal's `org.gnome.desktop.interface`.
    fn portal_key(self) -> &'static str {
        match self {
            Self::Ui => "font-name",
            Self::Mono => "monospace-font-name",
        }
    }

    /// Its fontconfig generic family.
    fn generic_family(self) -> &'static str {
        match self {
            Self::Ui => "sans-serif",
            Self::Mono => "monospace",
        }
    }
}

/// A desktop font: its family, and its text size in pixels when the setting
/// gives one.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct DesktopFont {
    family: String,
    size_px: Option<f32>,
}

impl DesktopFont {
    /// The theme rem that makes labels the desktop's text size.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn rem_px(&self) -> Option<f32> {
        self.size_px
            .map(|size| size.clamp(TEXT_PX.0, TEXT_PX.1) / LABEL_REMS)
    }
}

/// A Pango font description, as GNOME writes `font-name`: `"Cantarell 11"`,
/// `"SF Pro Display  10"`, `"Noto Sans Bold Italic 10"`, `"Inter, Noto Sans
/// 11px"`. The size is points unless it ends in `px`; the style words before
/// it go; of a family list, the first family stays.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_pango(description: &str) -> Option<DesktopFont> {
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
    let size_px = words.last().and_then(|last| match last.strip_suffix("px") {
        Some(pixels) => pixels.parse::<f32>().ok(),
        None => last.parse::<f32>().ok().map(|points| points * PX_PER_PT),
    });
    if size_px.is_some() {
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
    (!family.is_empty()).then(|| DesktopFont {
        family: family.to_owned(),
        size_px: size_px.filter(|size| size.is_finite() && *size > 0.),
    })
}

/// A Qt font string, as KDE writes `font` and `fixed` in `kdeglobals`:
/// `"SF Pro Display,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1"`: family, point
/// size, then pixel size, each `-1` when unset.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_qt(value: &str) -> Option<DesktopFont> {
    let mut fields = value.split(',').map(str::trim);
    let family = fields.next().filter(|family| !family.is_empty())?;
    let positive = |field: Option<&str>| {
        field
            .and_then(|field| field.parse::<f32>().ok())
            .filter(|size| size.is_finite() && *size > 0.)
    };
    let points = positive(fields.next());
    let pixels = positive(fields.next());
    Some(DesktopFont {
        family: family.to_owned(),
        size_px: points.map(|points| points * PX_PER_PT).or(pixels),
    })
}

/// `key` from the `[General]` group of a `kdeglobals` file.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn kde_general_font(kdeglobals: &str, key: &str) -> Option<DesktopFont> {
    let mut in_general = false;
    for line in kdeglobals.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_general = line == "[General]";
        } else if in_general
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == key
        {
            return parse_qt(value);
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
    use gpui::{App, AppContext as _, SharedString, font, px};
    use gpui_component::Theme;

    use super::{DesktopFont, Role, installed, kde_general_font, parse_pango};

    const PORTAL_NAMESPACE: &str = "org.gnome.desktop.interface";

    pub(super) fn apply(cx: &mut App) {
        let kdeglobals = dirs::config_dir()
            .and_then(|dir| std::fs::read_to_string(dir.join("kdeglobals")).ok())
            .unwrap_or_default();
        for role in Role::ALL {
            let first = kde_general_font(&kdeglobals, role.kde_key()).or_else(|| {
                generic(role, cx).map(|family| DesktopFont {
                    family,
                    size_px: None,
                })
            });
            if let Some(desktop) = first {
                set(role, &desktop, cx);
            }
        }
        let read = cx.background_spawn(portal_fonts());
        cx.spawn(async move |cx| {
            let fonts = read.await;
            cx.update(|cx| {
                for (role, desktop) in fonts {
                    set(role, &desktop, cx);
                }
            });
        })
        .detach();
    }

    /// Puts the desktop's family, when installed, and for the UI its size,
    /// when known, on the theme; the windows redraw if anything changed.
    fn set(role: Role, desktop: &DesktopFont, cx: &mut App) {
        let names = cx.text_system().all_font_names();
        let family =
            installed(&desktop.family, &names).map(|family| SharedString::from(family.to_owned()));
        if family.is_none() {
            tracing::info!(?role, family = %desktop.family, "the desktop's font isn't installed");
        }
        let theme = Theme::global(cx);
        let (current_family, rem) = match role {
            Role::Ui => (&theme.font_family, desktop.rem_px().map(px)),
            Role::Mono => (&theme.mono_font_family, None),
        };
        let family = family.filter(|family| family != current_family);
        let rem = rem.filter(|rem| *rem != theme.font_size);
        if family.is_none() && rem.is_none() {
            return;
        }
        tracing::info!(?role, family = ?family, rem = ?rem, "desktop font");
        Theme::update(cx, |theme| {
            if let Some(family) = family {
                match role {
                    Role::Ui => theme.font_family = family,
                    Role::Mono => theme.mono_font_family = family,
                }
            }
            if let Some(rem) = rem {
                theme.font_size = rem;
            }
        });
    }

    /// The installed family fontconfig gives for the role's generic family.
    fn generic(role: Role, cx: &App) -> Option<String> {
        let text_system = cx.text_system();
        let resolved = text_system
            .get_font_for_id(text_system.resolve_font(&font(role.generic_family())))
            .map(|font| font.family.to_string())?;
        installed(&resolved, &text_system.all_font_names()).map(str::to_owned)
    }

    /// The fonts the settings portal names, for the roles it answers.
    async fn portal_fonts() -> Vec<(Role, DesktopFont)> {
        let Ok(settings) = ashpd::desktop::settings::Settings::new().await else {
            return Vec::new();
        };
        let mut fonts = Vec::new();
        for role in Role::ALL {
            if let Ok(description) = settings
                .read::<String>(PORTAL_NAMESPACE, role.portal_key())
                .await
                && let Some(desktop) = parse_pango(&description)
            {
                fonts.push((role, desktop));
            }
        }
        fonts
    }
}

#[cfg(test)]
mod tests;
