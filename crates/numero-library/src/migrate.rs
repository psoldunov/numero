//! The one-time move from the first layout, `<hex id>.numi` files and an
//! `index.json` in the folder, to named `.txt` files.
//!
//! Each legacy file becomes a `.txt` named after its first line, in the
//! legacy index's order (files it doesn't list after, newest id first).
//! The legacy files then go to `<state dir>/legacy-backup/`, never deleted.
//! A file that fails to convert stays where it is, so the next start tries
//! it again; one that converted is gone from the folder, so a second run
//! has nothing to do.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::files;
use crate::id::CalculationId;
use crate::naming::{self, Taken};
use crate::state;

const LEGACY_EXTENSION: &str = "numi";
const LEGACY_INDEX: &str = "index.json";
const BACKUP: &str = "legacy-backup";
/// Legacy ids were milliseconds since 1970 in lowercase hex.
const MAX_LEGACY_ID: usize = 32;

#[derive(Debug, Default, Deserialize)]
struct LegacyIndex {
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    last_open: Option<String>,
}

/// Converts the legacy calculations in `folder`, if it has any, and
/// returns their new names in order (newest first) and the one open last.
pub(crate) fn run(folder: &Path, state_dir: &Path) -> Option<state::Folder> {
    let legacy = legacy_ids(folder);
    let index_path = folder.join(LEGACY_INDEX);
    let index = read_index(&index_path);
    if legacy.is_empty() && index.is_none() {
        return None;
    }
    tracing::info!(count = legacy.len(), folder = %folder.display(), "moving calculations to named text files");
    let index = index.unwrap_or_default();
    let mut taken = Taken::new(files::names(folder).iter().map(String::as_str));
    let mut renamed: Vec<(String, CalculationId)> = Vec::new();
    for hex in legacy_order(&legacy, &index) {
        match convert(folder, state_dir, &hex, &mut taken) {
            Ok(id) => renamed.push((hex, id)),
            Err(error) => {
                tracing::error!(%error, id = hex, "couldn't convert a calculation; it stays as it is");
            }
        }
    }
    if renamed.len() == legacy.len()
        && index_path.exists()
        && let Err(error) = back_up(state_dir, &index_path)
    {
        tracing::error!(%error, "couldn't move the old index out of the folder");
    }
    let last_open = index.last_open.and_then(|last| {
        renamed
            .iter()
            .find(|(hex, _)| *hex == last)
            .map(|(_, id)| id.clone())
    });
    Some(state::Folder {
        order: renamed.into_iter().map(|(_, id)| id).collect(),
        last_open,
    })
}

/// Writes the legacy calculation `hex` as a named `.txt`, then moves the
/// legacy file to the backup; undoes the first if the second fails.
fn convert(
    folder: &Path,
    state_dir: &Path,
    hex: &str,
    taken: &mut Taken,
) -> io::Result<CalculationId> {
    let source = folder.join(format!("{hex}.{LEGACY_EXTENSION}"));
    let text = fs::read_to_string(&source)?;
    let id = naming::unique(&naming::stem_for(&text), taken, naming::numbered)?;
    files::write_atomic(folder, &id, &text)?;
    if let Err(error) = back_up(state_dir, &source) {
        if let Err(undo) = fs::remove_file(folder.join(id.as_str())) {
            tracing::error!(error = %undo, id = %id, "couldn't undo a half-done conversion");
        }
        return Err(error);
    }
    taken.insert(&id);
    Ok(id)
}

/// Moves `file` into `<state dir>/legacy-backup/`, beside any earlier one
/// of the same name rather than over it.
fn back_up(state_dir: &Path, file: &Path) -> io::Result<()> {
    let backup = state_dir.join(BACKUP);
    fs::create_dir_all(&backup)?;
    let name = file
        .file_name()
        .ok_or_else(|| io::Error::other(format!("not a file: {}", file.display())))?;
    let target = free_path(&backup.join(name))?;
    files::move_file(file, &target)
}

fn free_path(wanted: &Path) -> io::Result<PathBuf> {
    if !wanted.try_exists()? {
        return Ok(wanted.to_owned());
    }
    for attempt in 1..1000 {
        let mut name = wanted.as_os_str().to_owned();
        name.push(format!(".{attempt}"));
        let candidate = PathBuf::from(name);
        if !candidate.try_exists()? {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free backup name for {}", wanted.display()),
    ))
}

fn legacy_ids(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != LEGACY_EXTENSION || !entry.file_type().ok()?.is_file() {
                return None;
            }
            let stem = path.file_stem()?.to_str()?;
            is_legacy_id(stem).then(|| stem.to_owned())
        })
        .collect()
}

fn is_legacy_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_LEGACY_ID
        && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `None` when there's no index, or it isn't the legacy one.
fn read_index(path: &Path) -> Option<LegacyIndex> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes)
        .inspect_err(|error| tracing::warn!(%error, path = %path.display(), "an index.json that isn't Numero's old one; leaving it"))
        .ok()
}

/// The index's order, then the files it doesn't list, newest id first.
fn legacy_order(legacy: &[String], index: &LegacyIndex) -> Vec<String> {
    let present: HashSet<&str> = legacy.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let listed: Vec<String> = index
        .order
        .iter()
        .filter(|hex| present.contains(hex.as_str()) && seen.insert(hex.as_str()))
        .cloned()
        .collect();
    let mut unlisted: Vec<String> = legacy
        .iter()
        .filter(|hex| !seen.contains(hex.as_str()))
        .cloned()
        .collect();
    unlisted.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| b.cmp(a)));
    listed.into_iter().chain(unlisted).collect()
}
