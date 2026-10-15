//! The menu-bar item on macOS: an `NSStatusItem` through `tray-icon`, as
//! concierge's (`crates/concierge-app/src/tray/macos.rs`). The glyph is a
//! template image, so AppKit draws it in the menu bar's own colour. A left
//! click shows or hides the window and a right click opens the menu, as
//! Numi's `StatusItemButton` does.
//!
//! AppKit owns the status item, so it is made on the main thread and lives
//! in the app's state until menu-bar mode is turned off.

use async_channel::Sender;
use gpui::App;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{TITLE, TrayEvent, TrayItem, glyph};
use crate::system::Event;

/// The icon's side in pixels: the menu bar's 22 points at twice the scale.
const ICON_SIZE: u32 = 44;

/// How much of the side the glyph leaves clear on every side, so it stands
/// about 16 points tall, the weight of the system's own items.
const ICON_MARGIN: f64 = 3. / 22.;

/// The status item, shown for as long as this lives.
pub(in crate::system) struct Tray {
    _item: Option<TrayIcon>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Tray")
    }
}

impl Tray {
    /// Puts the item in the menu bar with `menu`, the menu's entries, and
    /// reports to `events` whether it shows and what the user does with it.
    /// Call it on the main thread.
    pub(in crate::system) fn start(
        menu: Vec<Option<(&'static str, TrayItem)>>,
        events: Sender<Event>,
        _cx: &App,
    ) -> Self {
        match build(&menu, &events) {
            Ok(item) => {
                tracing::info!("the menu-bar item is up");
                send(&events, TrayEvent::Shown);
                Self { _item: Some(item) }
            }
            Err(error) => {
                tracing::warn!(%error, "couldn't put up the menu-bar item");
                send(&events, TrayEvent::Unavailable(error));
                Self { _item: None }
            }
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    }
}

fn send(events: &Sender<Event>, event: TrayEvent) {
    // The app quitting closes the channel; the item goes with it.
    let _gone = events.try_send(Event::Tray(event));
}

fn build(
    entries: &[Option<(&'static str, TrayItem)>],
    events: &Sender<Event>,
) -> Result<TrayIcon, String> {
    let menu = Menu::new();
    let mut items: Vec<(MenuId, TrayItem)> = Vec::new();
    for entry in entries {
        let appended = match *entry {
            Some((label, item)) => {
                let menu_item = MenuItem::new(label, true, None);
                items.push((menu_item.id().clone(), item));
                menu.append(&menu_item)
            }
            None => menu.append(&PredefinedMenuItem::separator()),
        };
        appended.map_err(|error| error.to_string())?;
    }
    let item = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_menu_on_right_click(true)
        .with_icon_templated(icon()?)
        .with_tooltip(TITLE)
        .build()
        .map_err(|error| error.to_string())?;
    let chosen = events.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some((_, item)) = items.iter().find(|(id, _)| *id == event.id) {
            send(&chosen, TrayEvent::Chose(*item));
        }
    }));
    let clicked = events.clone();
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            send(&clicked, TrayEvent::Toggle);
        }
    }));
    Ok(item)
}

/// The glyph, black where it covers and clear elsewhere, for a template
/// image.
fn icon() -> Result<Icon, String> {
    let rgba = glyph(ICON_SIZE, ICON_MARGIN)
        .into_iter()
        .flat_map(|alpha| [0, 0, 0, alpha])
        .collect();
    Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).map_err(|error| error.to_string())
}
