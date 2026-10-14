//! The global shortcut's recorder: a click starts it, the next key
//! combination becomes the shortcut, Esc gives up and ✕ clears it. The
//! shortcut is kept as a GPUI keystroke (`ctrl-alt-space`), which the
//! system module registers with the desktop.
//!
//! The recorder takes keystrokes before GPUI matches them to bindings
//! (`App::intercept_keystrokes`), so recording Ctrl+Q doesn't quit, as in
//! concierge's Hotkeys pane.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, Keystroke, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex};

use super::Preferences;

/// What a keystroke does to a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Esc: stop, keeping the shortcut as it was.
    Cancel,
    /// A key that can't be a global shortcut, such as a plain letter.
    Refuse,
    /// A modifier on its own: wait for the rest.
    Wait,
    /// The new shortcut.
    Take(String),
}

/// What `keystroke` does to a recording.
pub(crate) fn outcome(keystroke: &Keystroke) -> Outcome {
    let held = &keystroke.modifiers;
    let key = keystroke.key.as_str();
    if key == "escape" && !held.modified() {
        return Outcome::Cancel;
    }
    if key.is_empty() || matches!(key, "shift" | "control" | "alt" | "platform" | "function") {
        return Outcome::Wait;
    }
    let function_key = key.len() > 1 && key.starts_with('f') && key[1..].parse::<u8>().is_ok();
    if !(held.control || held.alt || held.platform || function_key) {
        return Outcome::Refuse;
    }
    let pressed = Keystroke {
        modifiers: *held,
        key: key.to_owned(),
        key_char: None,
    };
    Outcome::Take(pressed.unparse())
}

/// A shortcut as the platform writes one: `⌃⌥Space` on macOS,
/// `Ctrl+Alt+Space` elsewhere.
pub(crate) fn describe(hotkey: &str) -> String {
    let Ok(keystroke) = Keystroke::parse(hotkey) else {
        return hotkey.to_owned();
    };
    let held = keystroke.modifiers;
    let key = key_name(&keystroke.key);
    if cfg!(target_os = "macos") {
        let marks = [
            (held.control, "⌃"),
            (held.alt, "⌥"),
            (held.shift, "⇧"),
            (held.platform, "⌘"),
        ];
        let prefix: String = marks
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, mark)| *mark)
            .collect();
        format!("{prefix}{key}")
    } else {
        let names = [
            (held.control, "Ctrl"),
            (held.alt, "Alt"),
            (held.shift, "Shift"),
            (held.platform, "Super"),
        ];
        let mut parts: Vec<String> = names
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| (*name).to_owned())
            .collect();
        parts.push(key);
        parts.join("+")
    }
}

fn key_name(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The recording under way, if any.
#[derive(Default)]
pub(super) struct Recorder {
    recording: Option<Recording>,
}

struct Recording {
    refused: bool,
    _intercept: Subscription,
}

impl Recorder {
    pub(super) fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Gives up, keeping the shortcut as it was.
    pub(super) fn stop(&mut self, cx: &mut Context<'_, Preferences>) {
        if self.recording.take().is_some() {
            cx.notify();
        }
    }
}

impl Preferences {
    /// Waits for the next key combination in this window.
    pub(super) fn start_recording(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        window.focus(&self.focus, cx);
        let here = window.window_handle();
        let view = cx.entity().downgrade();
        let intercept = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != here {
                return;
            }
            let taken = view
                .update(cx, |this, cx| this.recorded(&event.keystroke, cx))
                .unwrap_or(false);
            if taken {
                cx.stop_propagation();
            }
        });
        self.recorder.recording = Some(Recording {
            refused: false,
            _intercept: intercept,
        });
        cx.notify();
    }

    /// Takes `keystroke` for the recording; true when it was the
    /// recorder's.
    pub(super) fn recorded(&mut self, keystroke: &Keystroke, cx: &mut Context<'_, Self>) -> bool {
        let Some(recording) = &mut self.recorder.recording else {
            return false;
        };
        match outcome(keystroke) {
            Outcome::Wait => false,
            Outcome::Refuse => {
                recording.refused = true;
                cx.notify();
                true
            }
            Outcome::Cancel => {
                self.recorder.stop(cx);
                true
            }
            Outcome::Take(hotkey) => {
                self.recorder.recording = None;
                self.update(|settings| settings.hotkey = Some(hotkey), cx);
                cx.notify();
                true
            }
        }
    }

    /// The recorder's button and the ✕ that clears the shortcut.
    pub(super) fn recorder(&self, cx: &mut Context<'_, Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (border, hover, muted, ring) = (
            theme.border,
            theme.secondary_hover,
            theme.muted_foreground,
            theme.ring,
        );
        let hotkey = self.settings(cx).hotkey;
        let recording = self.recorder.recording.as_ref();
        let label = match (recording, &hotkey) {
            (Some(recording), _) if recording.refused => {
                "Hold Ctrl, Alt or ⌘ with the key".to_owned()
            }
            (Some(_), _) => "Press a shortcut · Esc cancels".to_owned(),
            (None, Some(hotkey)) => describe(hotkey),
            (None, None) => "Click to record".to_owned(),
        };
        h_flex()
            .gap_1()
            .child(
                div()
                    .id("hotkey-recorder")
                    .min_w(px(150.))
                    .px_2()
                    .py(px(3.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if recording.is_some() { ring } else { border })
                    .text_center()
                    .cursor_pointer()
                    .when(hotkey.is_none() && recording.is_none(), |field| {
                        field.text_color(muted)
                    })
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.recorder.is_recording() {
                            this.recorder.stop(cx);
                        } else {
                            this.start_recording(window, cx);
                        }
                    }))
                    .child(label),
            )
            .when(hotkey.is_some(), |row| {
                row.child(
                    div()
                        .id("hotkey-clear")
                        .p_1()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .text_color(muted)
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recorder.stop(cx);
                            this.update(|settings| settings.hotkey = None, cx);
                        }))
                        .child(Icon::new(IconName::Close).xsmall()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(source: &str) -> Outcome {
        outcome(&Keystroke::parse(source).unwrap())
    }

    #[test]
    fn a_combination_with_a_modifier_becomes_the_shortcut() {
        assert_eq!(
            press("ctrl-alt-space"),
            Outcome::Take("ctrl-alt-space".into())
        );
        assert_eq!(press("ctrl-shift-k"), Outcome::Take("ctrl-shift-k".into()));
        assert_eq!(press("f5"), Outcome::Take("f5".into()));
    }

    #[test]
    fn escape_cancels_and_plain_keys_are_refused() {
        assert_eq!(press("escape"), Outcome::Cancel);
        assert_eq!(press("k"), Outcome::Refuse);
        assert_eq!(press("shift-k"), Outcome::Refuse);
    }

    #[test]
    fn a_shortcut_reads_as_the_platform_writes_it() {
        let shown = describe("ctrl-alt-space");
        if cfg!(target_os = "macos") {
            assert_eq!(shown, "⌃⌥Space");
        } else {
            assert_eq!(shown, "Ctrl+Alt+Space");
        }
    }
}
