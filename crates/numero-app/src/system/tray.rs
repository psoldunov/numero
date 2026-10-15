//! Numi's menu-bar mode: an item in the tray on Linux, over
//! `StatusNotifierItem`, and in the menu bar on macOS, an `NSStatusItem`.
//! A left click shows or hides the window; a right click opens the menu
//! (`docs/numi-spec-ui.md` §6.1). Numi's licence and update items are left
//! out.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub(super) use linux::Tray;
#[cfg(target_os = "macos")]
pub(super) use macos::Tray;

/// What the tray tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TrayEvent {
    /// The desktop shows the item.
    Shown,
    /// The item couldn't be shown, or went away; the reason is for the log.
    Unavailable(String),
    /// The user clicked the item.
    Toggle,
    /// The user chose an item of its menu.
    Chose(TrayItem),
}

/// The tray menu's items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayItem {
    Import,
    Export,
    Print,
    CopyToClipboard,
    Docs,
    Preferences,
    Quit,
}

/// The menu, top to bottom: each item's label and what it does, `None` for
/// a separator. Numi's own order: Import, Export, Print, Copy to Clipboard;
/// Docs, Preferences; Quit.
pub(crate) const MENU: [Option<(&str, TrayItem)>; 9] = [
    Some(("Import\u{2026}", TrayItem::Import)),
    Some(("Export\u{2026}", TrayItem::Export)),
    Some(("Print\u{2026}", TrayItem::Print)),
    Some(("Copy to Clipboard", TrayItem::CopyToClipboard)),
    None,
    Some(("Docs", TrayItem::Docs)),
    Some(("Preferences\u{2026}", TrayItem::Preferences)),
    None,
    Some(("Quit Numero", TrayItem::Quit)),
];

/// What the tooltip and the item's title say.
const TITLE: &str = "Numero";

/// The menu as `available` leaves it: the items it offers, with no
/// separator at either end or two in a row.
pub(crate) fn menu(available: impl Fn(TrayItem) -> bool) -> Vec<Option<(&'static str, TrayItem)>> {
    let mut entries: Vec<Option<(&str, TrayItem)>> = Vec::new();
    for entry in MENU {
        match entry {
            Some((_, item)) if !available(item) => {}
            None if matches!(entries.last(), None | Some(None)) => {}
            entry => entries.push(entry),
        }
    }
    if entries.last() == Some(&None) {
        entries.pop();
    }
    entries
}

/// The app icon's #, as an alpha mask `size` pixels square: the four
/// leaning bars of `assets/linux/icons/hicolor/symbolic/apps/
/// numero-symbolic.svg`, in its 232 to 792 view, with `margin` of the side
/// left clear all round. Drawn here so the tray needs no icon theme and the
/// menu bar gets a template image. The bars' 16-unit corner rounding is
/// under a pixel at tray sizes, so their corners are sharp here.
pub(crate) fn glyph(size: u32, margin: f64) -> Vec<u8> {
    /// The two uprights and the two crossbars, each a parallelogram leaning
    /// 11°, corners clockwise from the top left, in the icon's units.
    const BARS: [[(f64, f64); 4]; 4] = [
        [(404.1, 244.), (512.1, 244.), (407.9, 780.), (299.9, 780.)],
        [(616.1, 244.), (724.1, 244.), (619.9, 780.), (511.9, 780.)],
        [(311.5, 360.), (771.5, 360.), (751.3, 464.), (291.3, 464.)],
        [(272.6, 560.), (732.6, 560.), (712.4, 664.), (252.4, 664.)],
    ];
    /// The symbolic icon's view: a square from 232 to 792.
    const VIEW: (f64, f64) = (232., 560.);
    let side = f64::from(size);
    let scale = side * margin.mul_add(-2., 1.) / VIEW.1;
    let offset = (side - VIEW.1 * scale) / 2.;
    let mut alpha = Vec::with_capacity((size * size) as usize);
    for row in 0..size {
        for column in 0..size {
            // The pixel's centre, in the icon's units.
            let x = (f64::from(column) + 0.5 - offset) / scale + VIEW.0;
            let y = (f64::from(row) + 0.5 - offset) / scale + VIEW.0;
            let outside = BARS
                .iter()
                .map(|bar| outside_of(bar, (x, y)))
                .fold(f64::INFINITY, f64::min);
            // In pixels, from the bar's edge: inside is coverage.
            let inside = 0.5 - outside * scale;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0 to 255 first"
            )]
            alpha.push((inside.clamp(0., 1.) * 255.).round() as u8);
        }
    }
    alpha
}

/// How far `point` is outside the convex polygon `corners`, listed
/// clockwise (y down): negative inside. Exact along the edges, which is
/// what antialiasing reads.
fn outside_of(corners: &[(f64, f64); 4], point: (f64, f64)) -> f64 {
    (0..corners.len())
        .map(|index| {
            let from = corners[index];
            let to = corners[(index + 1) % corners.len()];
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            // The edge's outward normal, for a clockwise polygon with y
            // pointing down.
            let (nx, ny) = (dy, -dx);
            let length = nx.hypot(ny);
            ((point.0 - from.0) * nx + (point.1 - from.1) * ny) / length
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_follows_numis_order_and_ends_with_quit() {
        let labels: Vec<&str> = MENU.iter().flatten().map(|(label, _)| *label).collect();
        assert_eq!(
            labels,
            [
                "Import\u{2026}",
                "Export\u{2026}",
                "Print\u{2026}",
                "Copy to Clipboard",
                "Docs",
                "Preferences\u{2026}",
                "Quit Numero"
            ]
        );
        assert_eq!(MENU.last(), Some(&Some(("Quit Numero", TrayItem::Quit))));
    }

    #[test]
    fn items_the_app_cant_do_yet_leave_no_stray_separator() {
        let without = menu(|item| !matches!(item, TrayItem::Docs | TrayItem::Preferences));
        let shape: Vec<Option<TrayItem>> = without
            .iter()
            .map(|entry| entry.map(|(_, item)| item))
            .collect();
        assert_eq!(
            shape,
            [
                Some(TrayItem::Import),
                Some(TrayItem::Export),
                Some(TrayItem::Print),
                Some(TrayItem::CopyToClipboard),
                None,
                Some(TrayItem::Quit),
            ]
        );
        assert_eq!(menu(|_| true).len(), MENU.len());
        assert!(menu(|_| false).is_empty());
    }

    #[test]
    fn the_glyph_fills_its_square_and_stays_inside_the_margin() {
        let size = 22;
        let alpha = glyph(size, 1. / 11.);
        assert_eq!(alpha.len(), 22 * 22);
        assert!(alpha.contains(&u8::MAX), "some pixels are solid");
        assert!(alpha.contains(&0), "some pixels are clear");
        // The outermost row and column are clear.
        let edge =
            (0..size).flat_map(|i| [i, (size - 1) * size + i, i * size, i * size + size - 1]);
        assert!(edge.into_iter().all(|index| alpha[index as usize] == 0));
    }

    #[test]
    fn a_point_in_a_bar_is_inside_and_one_beside_it_is_not() {
        let bar = [(0., 0.), (10., 0.), (10., 10.), (0., 10.)];
        assert!((outside_of(&bar, (5., 5.)) + 5.).abs() < 1e-9);
        assert!((outside_of(&bar, (13., 5.)) - 3.).abs() < 1e-9);
    }

    #[test]
    fn the_glyph_is_a_hash_with_a_hole_in_its_middle() {
        let size = 64;
        let alpha = glyph(size, 0.);
        let at = |x: u32, y: u32| alpha[(y * size + x) as usize];
        // The middle of the # is the clear square between its bars.
        assert_eq!(at(size / 2, size / 2), 0);
        // The crossing of the left upright and the top crossbar is solid.
        assert_eq!(at(22, 21), u8::MAX);
    }
}
