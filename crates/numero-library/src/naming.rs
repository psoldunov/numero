//! File names for new calculations, from their first line.

use std::collections::HashSet;
use std::io;

use crate::id::{CalculationId, EXTENSION, MAX_NAME_BYTES, fold};
use crate::title;

/// The most characters of the first line a name keeps.
const MAX_STEM_CHARS: usize = 60;
/// The name of a calculation whose first line gives none.
const UNTITLED: &str = "Untitled";
/// Punctuation a name keeps, besides letters, digits and spaces.
const KEPT_PUNCTUATION: &str = "-_.,()'&";
/// How many numbered names to try before giving up.
const MAX_ATTEMPTS: u32 = 10_000;

/// The name, without `.txt`, a new calculation holding `text` gets: its
/// first line, less a Markdown heading's `#`s and anything a file name on
/// some system can't hold, or `Untitled`.
pub(crate) fn stem_for(text: &str) -> String {
    let line = title(text).unwrap_or_default();
    let line = line.trim_start_matches(|c: char| c == '#' || c.is_whitespace());
    let kept: String = line
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                Some(' ')
            } else if c.is_alphanumeric() || KEPT_PUNCTUATION.contains(c) {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let capped: String = collapsed.chars().take(MAX_STEM_CHARS).collect();
    // No leading dot (hidden) nor trailing one (Windows drops it).
    let tidy = capped.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if tidy.is_empty() {
        UNTITLED.to_owned()
    } else {
        tidy.to_owned()
    }
}

/// `Name`, `Name 2`, `Name 3`…: for new calculations and moved ones.
pub(crate) fn numbered(attempt: u32) -> String {
    if attempt == 1 {
        String::new()
    } else {
        format!(" {attempt}")
    }
}

/// `Name (conflict)`, `Name (conflict 2)`…: for a save that found the file
/// changed on disk.
pub(crate) fn conflict(attempt: u32) -> String {
    if attempt == 1 {
        " (conflict)".to_owned()
    } else {
        format!(" (conflict {attempt})")
    }
}

/// The names already used in a folder, compared without case, as macOS
/// volumes compare them.
#[derive(Debug, Default)]
pub(crate) struct Taken(HashSet<String>);

impl Taken {
    pub(crate) fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        Self(names.into_iter().map(fold).collect())
    }

    pub(crate) fn insert(&mut self, id: &CalculationId) {
        self.0.insert(id.folded());
    }

    fn contains(&self, name: &str) -> bool {
        self.0.contains(&fold(name))
    }
}

/// The first name `stem` + `suffix(n)` + `.txt` not in `taken`, for n = 1,
/// 2, 3…; the stem is shortened when the whole would be too long.
pub(crate) fn unique(
    stem: &str,
    taken: &Taken,
    suffix: impl Fn(u32) -> String,
) -> io::Result<CalculationId> {
    for attempt in 1..=MAX_ATTEMPTS {
        let suffix = suffix(attempt);
        let budget = MAX_NAME_BYTES.saturating_sub(suffix.len() + EXTENSION.len());
        let name = format!("{}{suffix}{EXTENSION}", truncate(stem, budget).trim_end());
        if taken.contains(&name) {
            continue;
        }
        return CalculationId::try_from(name).map_err(io::Error::other);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free name for {stem:?}"),
    ))
}

/// The longest start of `text` at most `max_bytes` long.
pub(crate) fn truncate(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let end = (0..=max_bytes)
        .rev()
        .find(|&end| text.is_char_boundary(end))
        .unwrap_or(0);
    &text[..end]
}
