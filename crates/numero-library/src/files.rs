//! Reading, writing and moving the files in a folder.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::time::SystemTime;

use crate::id::{CalculationId, MAX_NAME_BYTES};
use crate::naming::truncate;

/// A calculation's file as the folder lists it.
#[derive(Debug, Clone)]
pub(crate) struct OnDisk {
    pub id: CalculationId,
    pub modified: SystemTime,
}

/// Every calculation in `folder`: visible `.txt` files, not folders, not
/// links. Nothing when the folder can't be read.
pub(crate) fn calculations(folder: &Path) -> Vec<OnDisk> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::warn!(%error, folder = %folder.display(), "couldn't list the calculations");
            }
            return Vec::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let id = CalculationId::try_from(entry.file_name().to_str()?).ok()?;
            let metadata = entry.metadata().ok()?;
            metadata.is_file().then(|| OnDisk {
                id,
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect()
}

/// The name of everything in `folder`, so a new name clashes with none.
pub(crate) fn names(folder: &Path) -> Vec<String> {
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn read(folder: &Path, id: &CalculationId) -> io::Result<String> {
    fs::read_to_string(folder.join(id.as_str()))
}

/// Writes `text` to the calculation in one step: to a hidden temporary
/// file, which sync clients, Numi and [`calculations`] all pass over, then
/// renamed over the calculation.
pub(crate) fn write_atomic(folder: &Path, id: &CalculationId, text: &str) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    let temporary = folder.join(temporary_name(id.as_str()));
    let written = File::create(&temporary).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    let renamed = written.and_then(|()| fs::rename(&temporary, folder.join(id.as_str())));
    if renamed.is_err()
        && let Err(error) = fs::remove_file(&temporary)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %temporary.display(), "couldn't remove a temporary file");
    }
    renamed
}

/// `.Name.txt.tmp`: hidden and not `.txt`, and short enough to be a file
/// name.
fn temporary_name(name: &str) -> String {
    const DOT_AND_TMP: usize = ".".len() + ".tmp".len();
    format!(".{}.tmp", truncate(name, MAX_NAME_BYTES - DOT_AND_TMP))
}

/// Moves the file at `from` to `to`, which must not exist yet, across file
/// systems too.
pub(crate) fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if to.try_exists()? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => copy_then_remove(from, to),
        moved => moved,
    }
}

fn copy_then_remove(from: &Path, to: &Path) -> io::Result<()> {
    let name = to
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other(format!("not a file name: {}", to.display())))?;
    let temporary = to.with_file_name(temporary_name(name));
    let copied = fs::copy(from, &temporary).and_then(|_| fs::rename(&temporary, to));
    if let Err(error) = copied {
        // Best effort: the copy is incomplete and the original still there.
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::remove_file(from)
}
