//! The global shortcut on Linux, over the XDG `GlobalShortcuts` portal, as
//! concierge binds its own (its ADR 0020).
//!
//! KDE Plasma, GNOME 48 and later and Hyprland have the portal; the desktop
//! asks the user to confirm the keys once, then reports each press. Sway and
//! COSMIC have none: there the user binds `numero-app` to a key in the
//! compositor, and a second launch shows the running window. The portal
//! remembers the keys the user confirmed by the shortcut's id, so the id
//! carries the keys: a new shortcut in the settings is a new id, which the
//! desktop asks about afresh instead of keeping the old keys.
//!
//! A binding holds a D-Bus connection of its own. Dropping it drops the
//! connection, which ends the portal session and with it the shortcut.

use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use ashpd::{AppID, register_host_app_with_connection, zbus};
use async_channel::Sender;
use futures_util::StreamExt as _;
use gpui::{App, AppContext as _, Task};

use super::Hotkey;
use crate::app::APP_ID;
use crate::system::Event;

/// What the desktop's shortcut settings call it.
const DESCRIPTION: &str = "Show or hide Numero";

/// The shortcut, bound for as long as this lives.
pub(in crate::system) struct Binding {
    _task: Task<()>,
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Binding")
    }
}

impl Binding {
    /// Asks the desktop for `hotkey` and sends [`Event::Toggle`] on every
    /// press.
    pub(in crate::system) fn bind(hotkey: Hotkey, events: Sender<Event>, cx: &App) -> Self {
        let task = cx.background_spawn(async move {
            match hold(hotkey, &events).await {
                Ok(()) => {}
                Err(reason) => {
                    tracing::warn!(%reason, keys = hotkey.xdg_trigger(), "couldn't bind the global shortcut");
                }
            }
        });
        Self { _task: task }
    }
}

/// Binds `hotkey` and forwards its presses until the portal stops sending
/// them or nobody listens.
async fn hold(hotkey: Hotkey, events: &Sender<Event>) -> Result<(), String> {
    // A connection of its own: ashpd's shared one is GPUI's too.
    let connection = zbus::Connection::session()
        .await
        .map_err(|error| format!("the session bus can't be reached ({error})"))?;
    // An app outside Flatpak tells the portal who it is first. Portals older
    // than the Registry interface don't need it, so a failure isn't one.
    match APP_ID.parse::<AppID>() {
        Ok(app_id) => {
            if let Err(error) = register_host_app_with_connection(connection.clone(), app_id).await
            {
                tracing::debug!(%error, "the portal didn't take the app id");
            }
        }
        Err(error) => tracing::debug!(%error, "the app id isn't one the portal takes"),
    }
    let portal = GlobalShortcuts::with_connection(connection)
        .await
        .map_err(|error| format!("the desktop has no GlobalShortcuts portal ({error})"))?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|error| error.to_string())?;
    // Listen before binding, so no press is lost in between.
    let mut presses = portal
        .receive_activated()
        .await
        .map_err(|error| error.to_string())?;
    let id = shortcut_id(&hotkey);
    let trigger = hotkey.xdg_trigger();
    let wanted = [NewShortcut::new(id.as_str(), DESCRIPTION).preferred_trigger(trigger.as_str())];
    let bound = portal
        .bind_shortcuts(&session, &wanted, None, BindShortcutsOptions::default())
        .await
        .and_then(|request| request.response())
        .map_err(|error| error.to_string())?;
    let Some(shortcut) = bound
        .shortcuts()
        .iter()
        .find(|shortcut| shortcut.id() == id)
    else {
        return Err("the desktop bound no shortcut".to_owned());
    };
    tracing::info!(
        asked = %trigger,
        keys = shortcut.trigger_description(),
        "bound the global shortcut"
    );
    while let Some(press) = presses.next().await {
        // The signal isn't filtered by session; this connection holds only
        // this one.
        if press.shortcut_id() == id && events.send(Event::Toggle).await.is_err() {
            return Ok(());
        }
    }
    // The session lives as long as the shortcut should.
    drop(session);
    Err("the desktop stopped reporting shortcut presses".to_owned())
}

/// The id the portal keeps the shortcut under: its keys, so changing them
/// in Numero asks the desktop again.
fn shortcut_id(hotkey: &Hotkey) -> String {
    format!("toggle-window.{}", hotkey.keystroke())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_shortcut_has_an_id_of_its_own() {
        let first = Hotkey::parse("ctrl-alt-space").unwrap();
        let second = Hotkey::parse("ctrl-alt-n").unwrap();
        assert_eq!(shortcut_id(&first), "toggle-window.ctrl-alt-space");
        assert_ne!(shortcut_id(&first), shortcut_id(&second));
        assert_eq!(
            shortcut_id(&first),
            shortcut_id(&Hotkey::parse("alt-ctrl-space").unwrap())
        );
    }
}
