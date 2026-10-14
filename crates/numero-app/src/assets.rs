//! The icons the window draws: gpui-kit's default bundle, plus the four
//! Lucide icons it leaves out (ISC licence, `assets/icons/LICENSE-LUCIDE`).
//! As concierge's `ui/assets.rs`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_component::IconNamed;

/// The icons beyond gpui-kit's default bundle, by the path an `Icon` asks
/// for.
const EXTRA: [(&str, &[u8]); 4] = [
    ("icons/menu.svg", include_bytes!("../assets/icons/menu.svg")),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
    (
        "icons/arrow-right.svg",
        include_bytes!("../assets/icons/arrow-right.svg"),
    ),
    (
        "icons/trash.svg",
        include_bytes!("../assets/icons/trash.svg"),
    ),
];

/// The extra icons, for `Icon::new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    /// All calculations.
    Menu,
    Settings,
    /// Export.
    ArrowRight,
    /// Delete a calculation.
    Trash,
}

impl IconNamed for Glyph {
    fn path(self) -> SharedString {
        match self {
            Self::Menu => "icons/menu.svg",
            Self::Settings => "icons/settings.svg",
            Self::ArrowRight => "icons/arrow-right.svg",
            Self::Trash => "icons/trash.svg",
        }
        .into()
    }
}

/// gpui-kit's default assets, with the extra icons in front.
pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match EXTRA.iter().find(|(extra, _)| *extra == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut found = gpui::assets::Assets.list(path)?;
        found.extend(
            EXTRA
                .iter()
                .filter(|(extra, _)| extra.starts_with(path))
                .map(|(extra, _)| SharedString::from(*extra)),
        );
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_loads_and_the_default_bundle_still_does() {
        for glyph in [
            Glyph::Menu,
            Glyph::Settings,
            Glyph::ArrowRight,
            Glyph::Trash,
        ] {
            let path = glyph.path();
            assert!(Assets.load(&path).unwrap().is_some(), "{path}");
        }
        assert!(Assets.load("icons/plus.svg").unwrap().is_some());
    }
}
