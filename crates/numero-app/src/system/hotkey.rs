//! The global shortcut that shows and hides the window from any app.
//!
//! The settings keep it as a GPUI keystroke (`ctrl-alt-space`). Linux asks
//! the desktop for it over the XDG `GlobalShortcuts` portal, which wants the
//! XDG shortcuts notation (`CTRL+ALT+space`) and lets the desktop confirm
//! the keys; macOS registers it with Carbon's `RegisterEventHotKey` through
//! `global-hotkey`, which needs no Accessibility permission. Numi ships no
//! shortcut, and neither does Numero (`docs/numi-spec.md` §12.4).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub(super) use linux::Binding;
#[cfg(target_os = "macos")]
pub(super) use macos::Binding;

/// The modifiers a shortcut holds. `platform` is ⌘ on macOS and the Super
/// (logo) key on Linux.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

/// A key a global shortcut can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    /// `a` to `z`.
    Letter(char),
    /// `0` to `9`.
    Digit(char),
    /// F1 to F20.
    Function(u8),
    Named(Named),
}

/// The keys with a name rather than a character, and the punctuation keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Named {
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    Minus,
    Equal,
    Comma,
    Period,
    Slash,
    Semicolon,
    Quote,
    BracketLeft,
    BracketRight,
    Backslash,
    Backquote,
}

/// Each named key: GPUI's name for it and the XKB keysym name the portal
/// takes.
const NAMED: [(Named, &str, &str); 26] = [
    (Named::Space, "space", "space"),
    (Named::Enter, "enter", "Return"),
    (Named::Escape, "escape", "Escape"),
    (Named::Tab, "tab", "Tab"),
    (Named::Backspace, "backspace", "BackSpace"),
    (Named::Delete, "delete", "Delete"),
    (Named::Insert, "insert", "Insert"),
    (Named::Home, "home", "Home"),
    (Named::End, "end", "End"),
    (Named::PageUp, "pageup", "Page_Up"),
    (Named::PageDown, "pagedown", "Page_Down"),
    (Named::Up, "up", "Up"),
    (Named::Down, "down", "Down"),
    (Named::Left, "left", "Left"),
    (Named::Right, "right", "Right"),
    (Named::Minus, "-", "minus"),
    (Named::Equal, "=", "equal"),
    (Named::Comma, ",", "comma"),
    (Named::Period, ".", "period"),
    (Named::Slash, "/", "slash"),
    (Named::Semicolon, ";", "semicolon"),
    (Named::Quote, "'", "apostrophe"),
    (Named::BracketLeft, "[", "bracketleft"),
    (Named::BracketRight, "]", "bracketright"),
    (Named::Backslash, "\\", "backslash"),
    (Named::Backquote, "`", "grave"),
];

/// The highest function key a shortcut can be on.
const MAX_FUNCTION_KEY: u8 = 20;

/// A global shortcut: the modifiers held and the key pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hotkey {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Hotkey {
    /// Reads a GPUI keystroke, such as `ctrl-alt-space` or `cmd-shift-=`.
    /// Modifiers come in any order and any case; `secondary` is ⌘ on macOS
    /// and Ctrl elsewhere, as in GPUI's key bindings.
    pub(crate) fn parse(keystroke: &str) -> Result<Self, String> {
        let keystroke = keystroke.trim();
        let (held, key) = split_key(keystroke)
            .ok_or_else(|| format!("{keystroke:?} names no key for the shortcut"))?;
        let mut modifiers = Modifiers::default();
        for name in held {
            match name.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.control = true,
                "alt" | "option" | "opt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "cmd" | "super" | "win" | "platform" | "meta" | "logo" => {
                    modifiers.platform = true;
                }
                "secondary" if cfg!(target_os = "macos") => modifiers.platform = true,
                "secondary" => modifiers.control = true,
                "fn" => return Err("a global shortcut can't use the Fn key".to_owned()),
                other => return Err(format!("{other:?} isn't a modifier")),
            }
        }
        Ok(Self {
            modifiers,
            key: parse_key(key)?,
        })
    }

    /// The shortcut in the XDG shortcuts notation the portal takes:
    /// `CTRL+ALT+space`.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn xdg_trigger(&self) -> String {
        let Modifiers {
            control,
            alt,
            shift,
            platform,
        } = self.modifiers;
        [
            (control, "CTRL"),
            (alt, "ALT"),
            (shift, "SHIFT"),
            (platform, "LOGO"),
        ]
        .into_iter()
        .filter(|(held, _)| *held)
        .map(|(_, name)| name.to_owned())
        .chain([self.key.keysym()])
        .collect::<Vec<_>>()
        .join("+")
    }

    /// The shortcut as a GPUI keystroke, in GPUI's own order: what the
    /// portal remembers it by.
    pub(crate) fn keystroke(&self) -> String {
        let Modifiers {
            control,
            alt,
            shift,
            platform,
        } = self.modifiers;
        [
            (control, "ctrl"),
            (alt, "alt"),
            (platform, "super"),
            (shift, "shift"),
        ]
        .into_iter()
        .filter(|(held, _)| *held)
        .map(|(_, name)| name.to_owned())
        .chain([self.key.gpui_name()])
        .collect::<Vec<_>>()
        .join("-")
    }
}

/// Splits a keystroke into its modifiers and its key. The key may itself
/// be `-`, as in `ctrl--`.
fn split_key(keystroke: &str) -> Option<(Vec<&str>, &str)> {
    if keystroke.is_empty() {
        return None;
    }
    let (held, key) = if keystroke == "-" {
        ("", "-")
    } else if let Some(held) = keystroke.strip_suffix("--") {
        (held, "-")
    } else {
        keystroke.rsplit_once('-').unwrap_or(("", keystroke))
    };
    if key.is_empty() {
        return None;
    }
    let held = held.split('-').filter(|name| !name.is_empty()).collect();
    Some((held, key))
}

fn parse_key(key: &str) -> Result<Key, String> {
    let lower = key.to_ascii_lowercase();
    let mut characters = lower.chars();
    if let (Some(only), None) = (characters.next(), characters.next()) {
        if only.is_ascii_lowercase() {
            return Ok(Key::Letter(only));
        }
        if only.is_ascii_digit() {
            return Ok(Key::Digit(only));
        }
    }
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=MAX_FUNCTION_KEY).contains(&number)
    {
        return Ok(Key::Function(number));
    }
    NAMED
        .iter()
        .find(|(_, gpui, _)| *gpui == lower)
        .map(|(named, _, _)| Key::Named(*named))
        .ok_or_else(|| format!("{key:?} can't be a global shortcut's key"))
}

impl Key {
    /// The XKB keysym name of the key.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn keysym(self) -> String {
        match self {
            Self::Letter(letter) | Self::Digit(letter) => letter.to_string(),
            Self::Function(number) => format!("F{number}"),
            Self::Named(named) => NAMED
                .iter()
                .find(|(each, _, _)| *each == named)
                .map_or_else(String::new, |(_, _, keysym)| (*keysym).to_owned()),
        }
    }

    /// The key's W3C `KeyboardEvent.code`, which `global-hotkey` takes:
    /// `KeyA`, `Digit1`, `F5`, `Space`, `ArrowUp`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn code(self) -> String {
        match self {
            Self::Letter(letter) => format!("Key{}", letter.to_ascii_uppercase()),
            Self::Digit(digit) => format!("Digit{digit}"),
            Self::Function(number) => format!("F{number}"),
            Self::Named(named) => match named {
                Named::Up => "ArrowUp".to_owned(),
                Named::Down => "ArrowDown".to_owned(),
                Named::Left => "ArrowLeft".to_owned(),
                Named::Right => "ArrowRight".to_owned(),
                other => format!("{other:?}"),
            },
        }
    }

    /// GPUI's name for the key.
    fn gpui_name(self) -> String {
        match self {
            Self::Letter(letter) | Self::Digit(letter) => letter.to_string(),
            Self::Function(number) => format!("f{number}"),
            Self::Named(named) => NAMED
                .iter()
                .find(|(each, _, _)| *each == named)
                .map_or_else(String::new, |(_, gpui, _)| (*gpui).to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(keystroke: &str) -> Hotkey {
        Hotkey::parse(keystroke).unwrap()
    }

    #[test]
    fn a_keystroke_reads_as_its_modifiers_and_key() {
        let hotkey = parsed("ctrl-alt-space");
        assert_eq!(
            hotkey.modifiers,
            Modifiers {
                control: true,
                alt: true,
                ..Modifiers::default()
            }
        );
        assert_eq!(hotkey.key, Key::Named(Named::Space));
    }

    #[test]
    fn modifiers_come_in_any_order_and_case() {
        assert_eq!(parsed("Shift-CMD-n"), parsed("cmd-shift-n"));
        assert_eq!(parsed("super-shift-N").key, Key::Letter('n'));
        assert!(parsed("cmd-k").modifiers.platform);
        assert!(parsed("super-k").modifiers.platform);
    }

    #[test]
    fn the_minus_key_survives_the_dashes() {
        let hotkey = parsed("ctrl--");
        assert!(hotkey.modifiers.control);
        assert_eq!(hotkey.key, Key::Named(Named::Minus));
        assert_eq!(parsed("-").key, Key::Named(Named::Minus));
    }

    #[test]
    fn function_digit_and_punctuation_keys_parse() {
        assert_eq!(parsed("f5").key, Key::Function(5));
        assert_eq!(parsed("alt-F12").key, Key::Function(12));
        assert_eq!(parsed("ctrl-1").key, Key::Digit('1'));
        assert_eq!(parsed("ctrl-=").key, Key::Named(Named::Equal));
        assert_eq!(parsed("ctrl-pageup").key, Key::Named(Named::PageUp));
    }

    #[test]
    fn what_cant_be_a_shortcut_is_an_error() {
        assert!(Hotkey::parse("").is_err());
        assert!(Hotkey::parse("ctrl-").is_err());
        assert!(Hotkey::parse("hyper-a").is_err());
        assert!(Hotkey::parse("fn-a").is_err());
        assert!(Hotkey::parse("ctrl-f99").is_err());
        assert!(Hotkey::parse("ctrl-é").is_err());
        assert!(Hotkey::parse("ctrl-volumeup").is_err());
    }

    #[test]
    fn the_portal_gets_the_xdg_notation() {
        assert_eq!(parsed("ctrl-alt-space").xdg_trigger(), "CTRL+ALT+space");
        assert_eq!(parsed("shift-super-k").xdg_trigger(), "SHIFT+LOGO+k");
        assert_eq!(parsed("ctrl-enter").xdg_trigger(), "CTRL+Return");
        assert_eq!(parsed("alt-f4").xdg_trigger(), "ALT+F4");
        assert_eq!(parsed("ctrl--").xdg_trigger(), "CTRL+minus");
        assert_eq!(parsed("f8").xdg_trigger(), "F8");
    }

    #[test]
    fn the_keystroke_round_trips_in_gpuis_order() {
        assert_eq!(parsed("shift-ctrl-K").keystroke(), "ctrl-shift-k");
        assert_eq!(parsed("cmd-alt-space").keystroke(), "alt-super-space");
        assert_eq!(parsed(&parsed("ctrl--").keystroke()), parsed("ctrl--"));
    }

    #[test]
    fn macos_gets_the_w3c_key_codes() {
        assert_eq!(parsed("cmd-a").key.code(), "KeyA");
        assert_eq!(parsed("cmd-7").key.code(), "Digit7");
        assert_eq!(parsed("f13").key.code(), "F13");
        assert_eq!(parsed("ctrl-space").key.code(), "Space");
        assert_eq!(parsed("ctrl-up").key.code(), "ArrowUp");
        assert_eq!(parsed("ctrl-`").key.code(), "Backquote");
        assert_eq!(parsed("ctrl-pagedown").key.code(), "PageDown");
    }

    #[test]
    fn secondary_is_the_platforms_command_key() {
        let hotkey = parsed("secondary-space");
        if cfg!(target_os = "macos") {
            assert!(hotkey.modifiers.platform);
        } else {
            assert!(hotkey.modifiers.control);
        }
    }
}
