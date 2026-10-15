//! The global shortcut on macOS: registered with the system through
//! `RegisterEventHotKey`, which `global-hotkey` wraps and which needs no
//! Accessibility permission. The system hands presses to the main thread's
//! event loop, so the binding is made there and lives in the app's state.

use async_channel::Sender;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::App;

use super::Hotkey;
use crate::system::Event;

/// The shortcut, registered for as long as this lives.
pub(in crate::system) struct Binding {
    manager: Option<GlobalHotKeyManager>,
    hotkey: Option<HotKey>,
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Binding")
    }
}

impl Binding {
    /// Registers `hotkey` and sends [`Event::Toggle`] on every press. A key
    /// another app holds leaves the shortcut unbound, and says so in the
    /// log.
    pub(in crate::system) fn bind(hotkey: Hotkey, events: Sender<Event>, _cx: &App) -> Self {
        match register(hotkey) {
            Ok((manager, registered)) => {
                let id = registered.id();
                GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                    if event.id() == id && event.state() == HotKeyState::Pressed {
                        // The app quitting closes the channel.
                        let _gone = events.try_send(Event::Toggle);
                    }
                }));
                tracing::info!(keys = hotkey.keystroke(), "registered the global shortcut");
                Self {
                    manager: Some(manager),
                    hotkey: Some(registered),
                }
            }
            Err(reason) => {
                tracing::warn!(%reason, keys = hotkey.keystroke(), "couldn't register the global shortcut");
                Self {
                    manager: None,
                    hotkey: None,
                }
            }
        }
    }
}

impl Drop for Binding {
    fn drop(&mut self) {
        if let (Some(manager), Some(hotkey)) = (&self.manager, self.hotkey)
            && let Err(error) = manager.unregister(hotkey)
        {
            tracing::debug!(%error, "couldn't unregister the global shortcut");
        }
        GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
    }
}

fn register(hotkey: Hotkey) -> Result<(GlobalHotKeyManager, HotKey), String> {
    let code = hotkey
        .key
        .code()
        .parse::<Code>()
        .map_err(|_| format!("macOS has no key {}", hotkey.key.code()))?;
    let mut modifiers = Modifiers::empty();
    for (held, modifier) in [
        (hotkey.modifiers.control, Modifiers::CONTROL),
        (hotkey.modifiers.alt, Modifiers::ALT),
        (hotkey.modifiers.shift, Modifiers::SHIFT),
        (hotkey.modifiers.platform, Modifiers::SUPER),
    ] {
        if held {
            modifiers |= modifier;
        }
    }
    let registered = HotKey::new(Some(modifiers), code);
    let manager = GlobalHotKeyManager::new()
        .map_err(|error| format!("the system refused to watch for hotkeys ({error})"))?;
    manager
        .register(registered)
        .map_err(|error| format!("another app holds the keys ({error})"))?;
    Ok((manager, registered))
}
