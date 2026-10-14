//! The calculations on disk: one plain-text file each, and a small index
//! with their order and the one open last.
//!
//! Everything lives under `<data dir>/numero/calculations/`: `<id>.numi`
//! per calculation and `index.json`. The newest calculation comes first.
//! The index is a convenience: a missing or unreadable one is rebuilt from
//! the files, and a file the index doesn't know is added to the end, so no
//! calculation is lost to it. Ids read back from the index are checked, so a
//! hand-edited index can't name a file outside the directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// What the first run shows: Numi's own sample.
pub(crate) const SAMPLE: &str = "# Sample\n8 / (45 - 20%)\n5 inches in cm\n$9 in Euro";

const DIRECTORY: &str = "numero/calculations";
const INDEX: &str = "index.json";
const EXTENSION: &str = "numi";

/// A calculation's name on disk: lowercase hex digits only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct CalculationId(String);

impl CalculationId {
    fn new(value: u128) -> Self {
        Self(format!("{value:x}"))
    }

    fn is_valid(value: &str) -> bool {
        !value.is_empty() && value.len() <= 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

impl TryFrom<String> for CalculationId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(format!("not a calculation id: {value:?}"))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Index {
    /// Newest first.
    order: Vec<CalculationId>,
    last_open: Option<CalculationId>,
}

/// A calculation as the list of calculations shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub id: CalculationId,
    /// Its first non-empty line, if it has one.
    pub title: Option<String>,
    pub text: String,
}

impl Entry {
    /// Whether `query` appears in the calculation, ignoring case.
    pub(crate) fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.text.to_lowercase().contains(&query)
    }
}

/// The first non-empty line of `text`, trimmed.
pub(crate) fn title(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// The calculations under one directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Library {
    dir: PathBuf,
    index: Index,
}

impl Library {
    /// The library under the platform's data directory, or `None` when the
    /// platform has none.
    pub(crate) fn from_env() -> Option<Self> {
        dirs::data_dir().map(|dir| Self::open_in(&dir.join(DIRECTORY)))
    }

    /// The library in `dir`, its index read and squared with the files.
    pub(crate) fn open_in(dir: &Path) -> Self {
        let mut library = Self {
            dir: dir.to_owned(),
            index: read_index(dir),
        };
        library.reconcile();
        library
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// The calculation to open at start, and its text: the one open last,
    /// else the newest, else a new one holding the sample.
    pub(crate) fn open_last(&mut self) -> io::Result<(CalculationId, String)> {
        let candidates: Vec<CalculationId> = self
            .index
            .last_open
            .iter()
            .chain(self.index.order.iter())
            .cloned()
            .collect();
        for id in candidates {
            match self.load(&id) {
                Ok(text) => {
                    self.set_last_open(&id);
                    return Ok((id, text));
                }
                Err(error) => tracing::error!(%error, ?id, "couldn't read a calculation"),
            }
        }
        let id = self.create(SAMPLE)?;
        Ok((id, SAMPLE.to_owned()))
    }

    /// Adds a calculation holding `text` at the top, and makes it the open
    /// one.
    pub(crate) fn create(&mut self, text: &str) -> io::Result<CalculationId> {
        fs::create_dir_all(&self.dir)?;
        let mut id = CalculationId::new(now_millis());
        let mut bump = 0;
        while self.path(&id).exists() {
            bump += 1;
            id = CalculationId::new(now_millis() + bump);
        }
        self.save(&id, text)?;
        self.index.order.insert(0, id.clone());
        self.set_last_open(&id);
        Ok(id)
    }

    /// Writes `text` to the calculation atomically.
    pub(crate) fn save(&self, id: &CalculationId, text: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path(id);
        let temporary = path.with_extension("numi.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &path)
    }

    pub(crate) fn load(&self, id: &CalculationId) -> io::Result<String> {
        fs::read_to_string(self.path(id))
    }

    /// Removes the calculation's file and its place in the index.
    pub(crate) fn delete(&mut self, id: &CalculationId) -> io::Result<()> {
        match fs::remove_file(self.path(id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.index.order.retain(|kept| kept != id);
        if self.index.last_open.as_ref() == Some(id) {
            self.index.last_open = None;
        }
        self.write_index();
        Ok(())
    }

    /// Remembers `id` as the open calculation, for the next start.
    pub(crate) fn set_last_open(&mut self, id: &CalculationId) {
        if self.index.last_open.as_ref() != Some(id) {
            self.index.last_open = Some(id.clone());
            self.write_index();
        }
    }

    /// Every calculation, newest first, with its text. Files that can't be
    /// read are left out.
    pub(crate) fn entries(&self) -> Vec<Entry> {
        self.index
            .order
            .iter()
            .filter_map(|id| {
                let text = self.load(id).ok()?;
                Some(Entry {
                    id: id.clone(),
                    title: title(&text),
                    text,
                })
            })
            .collect()
    }

    fn path(&self, id: &CalculationId) -> PathBuf {
        self.dir.join(format!("{}.{EXTENSION}", id.0))
    }

    /// Drops ids whose file is gone and adds files the index doesn't know.
    fn reconcile(&mut self) {
        let on_disk = ids_on_disk(&self.dir);
        let before = self.index.clone();
        self.index.order.retain(|id| on_disk.contains(id));
        let mut unknown: Vec<CalculationId> = on_disk
            .into_iter()
            .filter(|id| !self.index.order.contains(id))
            .collect();
        unknown.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| b.0.cmp(&a.0)));
        self.index.order.extend(unknown);
        if let Some(last) = &self.index.last_open
            && !self.index.order.contains(last)
        {
            self.index.last_open = None;
        }
        if self.index != before {
            self.write_index();
        }
    }

    fn write_index(&self) {
        let written = serde_json::to_vec_pretty(&self.index)
            .map_err(io::Error::other)
            .and_then(|json| {
                fs::create_dir_all(&self.dir)?;
                let path = self.dir.join(INDEX);
                let temporary = path.with_extension("json.tmp");
                fs::write(&temporary, json)?;
                fs::rename(&temporary, &path)
            });
        if let Err(error) = written {
            tracing::error!(%error, "couldn't save the list of calculations");
        }
    }
}

fn read_index(dir: &Path) -> Index {
    let path = dir.join(INDEX);
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            tracing::error!(%error, path = %path.display(), "the list of calculations is unreadable; rebuilding it");
            Index::default()
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Index::default(),
        Err(error) => {
            tracing::error!(%error, path = %path.display(), "couldn't read the list of calculations");
            Index::default()
        }
    }
}

fn ids_on_disk(dir: &Path) -> Vec<CalculationId> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != EXTENSION {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_owned();
            CalculationId::try_from(stem).ok()
        })
        .collect()
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

#[cfg(test)]
mod tests;
