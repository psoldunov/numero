//! The tray item on Linux, over the `StatusNotifierItem` D-Bus protocol, as
//! concierge's (`crates/concierge-app/src/tray/linux.rs`).
//!
//! KDE Plasma, most wlroots bars and Waybar host it out of the box. GNOME
//! needs the `AppIndicator` extension; without a host the item can't
//! register, and the app says so in the log and keeps quitting when its
//! window closes, since nothing could bring it back.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ashpd::desktop::settings::{ColorScheme, Settings};
use async_channel::Sender;
use gpui::{App, AppContext as _, Task};
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Handle, Icon, OfflineReason, ToolTip, TrayMethods as _};

use super::{TITLE, TrayEvent, TrayItem, glyph};
use crate::system::Event;

/// The icon packaging installs, `hicolor/symbolic/apps/numero-symbolic.svg`.
const ICON_NAME: &str = "numero-symbolic";

/// The sizes the tray draws its icon at, in pixels; the panel picks one.
const ICON_SIZES: [u16; 6] = [16, 22, 24, 32, 48, 64];

/// How much of the icon's side the glyph leaves clear on every side.
const ICON_MARGIN: f64 = 1. / 16.;

/// How many times the item tries to register before it gives up: at login
/// the app can start before the panel that hosts tray items.
const ATTEMPTS: u32 = 6;

/// How long it waits between two attempts.
const PAUSE: Duration = Duration::from_secs(2);

/// The settings portal's namespace and key for Plasma's global theme, and
/// Breeze Twilight's: light apps, so the portal says light, on a dark
/// panel.
const KDE_GLOBALS: &str = "org.kde.kdeglobals.KDE";
const LOOK_AND_FEEL: &str = "LookAndFeelPackage";
const TWILIGHT: &str = "org.kde.breezetwilight.desktop";

/// The tray item, shown for as long as this lives.
pub(in crate::system) struct Tray {
    handle: Arc<Mutex<Option<Handle<NumeroTray>>>>,
    _registering: Task<()>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Tray")
    }
}

impl Tray {
    /// Registers the item with `menu`, the menu's entries, and reports to
    /// `events` whether it shows and what the user does with it.
    pub(in crate::system) fn start(
        menu: Vec<Option<(&'static str, TrayItem)>>,
        events: Sender<Event>,
        cx: &App,
    ) -> Self {
        let handle = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&handle);
        let executor = cx.background_executor().clone();
        let registering = cx.background_spawn(async move {
            let glyph = Glyph::for_desktop().await;
            for attempt in 1..=ATTEMPTS {
                let tray = NumeroTray {
                    events: events.clone(),
                    menu: menu.clone(),
                    glyph,
                };
                match tray.spawn().await {
                    Ok(spawned) => {
                        tracing::info!("the tray item is up");
                        *shared.lock().unwrap_or_else(PoisonError::into_inner) = Some(spawned);
                        send(&events, TrayEvent::Shown);
                        return;
                    }
                    Err(error) if attempt < ATTEMPTS => {
                        tracing::debug!(%error, attempt, "the tray item isn't up yet");
                        executor.timer(PAUSE).await;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "couldn't register the tray item");
                        send(&events, TrayEvent::Unavailable(error.to_string()));
                    }
                }
            }
        });
        Self {
            handle,
            _registering: registering,
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            // Asks the service to take the item down; it does so on its own
            // executor.
            drop(handle.shutdown());
            tracing::info!("took the tray item down");
        }
    }
}

fn send(events: &Sender<Event>, event: TrayEvent) {
    // The app quitting closes the channel; the item goes with it.
    let _gone = events.try_send(Event::Tray(event));
}

/// The colour the tray draws the glyph in, one that reads on the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
    /// For a dark panel.
    Light,
    /// For a light panel.
    Dark,
}

impl Glyph {
    /// Picks the colour for the panel of `desktop`, the value of
    /// `XDG_CURRENT_DESKTOP`, when it prefers `scheme` and Plasma's global
    /// theme is `look_and_feel` (empty elsewhere). Panels are dark unless
    /// the desktop asks for light. GNOME's top bar is dark whatever it asks
    /// for, and so is Breeze Twilight's panel.
    fn for_panel(scheme: ColorScheme, desktop: &str, look_and_feel: &str) -> Self {
        let gnome = desktop.split(':').any(|name| name == "GNOME");
        let twilight = look_and_feel == TWILIGHT;
        match scheme {
            ColorScheme::PreferLight if !gnome && !twilight => Self::Dark,
            _ => Self::Light,
        }
    }

    /// Asks the settings portal once. A panel the portal can't describe is
    /// taken as dark.
    async fn for_desktop() -> Self {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let Ok(settings) = Settings::new().await else {
            return Self::for_panel(ColorScheme::NoPreference, &desktop, "");
        };
        let scheme = settings
            .color_scheme()
            .await
            .unwrap_or(ColorScheme::NoPreference);
        let look_and_feel = settings
            .read::<String>(KDE_GLOBALS, LOOK_AND_FEEL)
            .await
            .unwrap_or_default();
        Self::for_panel(scheme, &desktop, &look_and_feel)
    }

    const fn rgb(self) -> [u8; 3] {
        match self {
            Self::Light => [0xff, 0xff, 0xff],
            Self::Dark => [0x1d, 0x1d, 0x1f],
        }
    }
}

struct NumeroTray {
    events: Sender<Event>,
    menu: Vec<Option<(&'static str, TrayItem)>>,
    glyph: Glyph,
}

impl ksni::Tray for NumeroTray {
    fn id(&self) -> String {
        crate::app::APP_ID.to_owned()
    }

    fn title(&self) -> String {
        TITLE.to_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: TITLE.to_owned(),
            ..ToolTip::default()
        }
    }

    /// The installed symbolic icon, which the panel colours itself; the
    /// pixmap stands in where it isn't installed.
    fn icon_name(&self) -> String {
        ICON_NAME.to_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        ICON_SIZES
            .iter()
            .map(|&size| icon(self.glyph.rgb(), size))
            .collect()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        send(&self.events, TrayEvent::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.menu
            .iter()
            .map(|entry| match *entry {
                Some((label, item)) => StandardItem {
                    label: label.to_owned(),
                    activate: Box::new(move |tray: &mut Self| {
                        send(&tray.events, TrayEvent::Chose(item));
                    }),
                    ..StandardItem::default()
                }
                .into(),
                None => MenuItem::Separator,
            })
            .collect()
    }

    // The panel restarting takes the item away until it comes back; keep
    // the service, and say so meanwhile.
    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        tracing::warn!(?reason, "the tray's host went away");
        send(
            &self.events,
            TrayEvent::Unavailable("the panel that shows tray items went away".to_owned()),
        );
        true
    }

    fn watcher_online(&self) {
        tracing::info!("the tray's host is back");
        send(&self.events, TrayEvent::Shown);
    }
}

/// The glyph in `[red, green, blue]`, in ARGB32 as the protocol wants it.
fn icon([red, green, blue]: [u8; 3], size: u16) -> Icon {
    let data = glyph(u32::from(size), ICON_MARGIN)
        .into_iter()
        .flat_map(|alpha| [alpha, red, green, blue])
        .collect();
    Icon {
        width: i32::from(size),
        height: i32::from(size),
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glyph_is_light_unless_the_desktop_prefers_light() {
        let breeze = "org.kde.breeze.desktop";
        assert_eq!(
            Glyph::for_panel(ColorScheme::PreferDark, "KDE", breeze),
            Glyph::Light
        );
        assert_eq!(
            Glyph::for_panel(ColorScheme::NoPreference, "KDE", breeze),
            Glyph::Light
        );
        assert_eq!(
            Glyph::for_panel(ColorScheme::PreferLight, "KDE", breeze),
            Glyph::Dark
        );
        assert_eq!(
            Glyph::for_panel(ColorScheme::PreferLight, "ubuntu:GNOME", ""),
            Glyph::Light
        );
        assert_eq!(
            Glyph::for_panel(ColorScheme::PreferLight, "KDE", TWILIGHT),
            Glyph::Light
        );
    }

    #[test]
    fn each_icon_is_a_square_of_argb_pixels_in_the_glyphs_colour() {
        let icon = icon(Glyph::Dark.rgb(), 22);
        assert_eq!((icon.width, icon.height), (22, 22));
        assert_eq!(icon.data.len(), 22 * 22 * 4);
        let opaque = icon.data.chunks(4).find(|pixel| pixel[0] == u8::MAX);
        assert_eq!(opaque, Some(&[u8::MAX, 0x1d, 0x1d, 0x1f][..]));
    }
}
