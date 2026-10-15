//! The calculations in one folder, in order, and what the library last saw
//! of each.

mod edit;
mod relocate;
mod rescan;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files;
use crate::id::CalculationId;
use crate::migrate;
use crate::state;
use crate::{Entry, default_folder, state_dir, title};

/// The calculations in one folder.
///
/// It holds the order of the list (newest first), the calculation open
/// last, and, per file, the text it last read or wrote: what it "knows".
/// A file whose text on disk differs from what it knows was changed by
/// someone else, and [`Library::rescan`] and [`Library::save`] act on that.
pub struct Library {
    folder: PathBuf,
    state_dir: PathBuf,
    /// The folder's key in the metadata file.
    key: String,
    /// Newest first.
    order: Vec<CalculationId>,
    last_open: Option<CalculationId>,
    /// The text last read from or written to each file.
    known: HashMap<CalculationId, String>,
    /// The text on disk last reported as modified, so it's reported once.
    reported: HashMap<CalculationId, String>,
}

impl fmt::Debug for Library {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Library")
            .field("folder", &self.folder)
            .field("state_dir", &self.state_dir)
            .field("order", &self.order)
            .field("last_open", &self.last_open)
            .finish_non_exhaustive()
    }
}

impl Library {
    /// The library in `folder`, keeping its order in `state_dir`. Creates
    /// the folder, converts a legacy layout found in it once, and squares
    /// the order with the files.
    pub fn open(folder: &Path, state_dir: &Path) -> Self {
        if let Err(error) = fs::create_dir_all(folder) {
            tracing::error!(%error, folder = %folder.display(), "couldn't create the calculations folder");
        }
        let key = state::key(folder);
        let stored = state::load(state_dir, &key);
        let mut library = Self {
            folder: folder.to_owned(),
            state_dir: state_dir.to_owned(),
            key,
            order: stored.order,
            last_open: stored.last_open,
            known: HashMap::new(),
            reported: HashMap::new(),
        };
        let migrated = migrate::run(folder, state_dir);
        if let Some(migrated) = &migrated {
            library.adopt(migrated);
        }
        let (added, removed) = library.reconcile();
        if migrated.is_some() || !added.is_empty() || !removed.is_empty() {
            library.persist();
        }
        library.remember_all();
        library
    }

    /// The library in `folder`, or in [`default_folder`] when `None`, with
    /// its order under [`state_dir`]. `None` when the platform has no data
    /// directory.
    pub fn from_env(folder: Option<&Path>) -> Option<Self> {
        let state = state_dir()?;
        let folder = match folder {
            Some(folder) => folder.to_owned(),
            None => default_folder()?,
        };
        Some(Self::open(&folder, &state))
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The calculation to open at start, and its text: the one open last,
    /// else the newest, else a new one holding `welcome`.
    pub fn open_last(&mut self, welcome: &str) -> io::Result<(CalculationId, String)> {
        let candidates: Vec<CalculationId> = self
            .last_open
            .iter()
            .chain(self.order.iter())
            .cloned()
            .collect();
        for id in candidates {
            match self.load(&id) {
                Ok(text) => {
                    self.set_last_open(&id);
                    return Ok((id, text));
                }
                Err(error) => tracing::error!(%error, %id, "couldn't read a calculation"),
            }
        }
        let id = self.create(welcome)?;
        Ok((id, welcome.to_owned()))
    }

    /// Remembers `id` as the open calculation, for the next start.
    pub fn set_last_open(&mut self, id: &CalculationId) {
        if self.last_open.as_ref() != Some(id) {
            self.last_open = Some(id.clone());
            self.persist();
        }
    }

    /// Every calculation, newest first, with its text as it is on disk.
    /// Files that can't be read are left out.
    pub fn entries(&self) -> Vec<Entry> {
        self.order
            .iter()
            .filter_map(|id| {
                let text = files::read(&self.folder, id)
                    .inspect_err(|error| tracing::warn!(%error, %id, "couldn't read a calculation"))
                    .ok()?;
                Some(Entry {
                    id: id.clone(),
                    title: title(&text),
                    text,
                })
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Puts `stored`'s calculations on top, in its order, and takes its
    /// last-open unless one is set already.
    fn adopt(&mut self, stored: &state::Folder) {
        let incoming: HashSet<&CalculationId> = stored.order.iter().collect();
        self.order = stored
            .order
            .iter()
            .cloned()
            .chain(
                self.order
                    .iter()
                    .filter(|id| !incoming.contains(id))
                    .cloned(),
            )
            .collect();
        if self.last_open.is_none() {
            self.last_open.clone_from(&stored.last_open);
        }
    }

    /// Squares the order with the folder: drops names whose file is gone
    /// (and repeats), and puts files it doesn't list on top, newest first.
    /// Returns what it added and removed.
    fn reconcile(&mut self) -> (Vec<CalculationId>, Vec<CalculationId>) {
        let mut on_disk = files::calculations(&self.folder);
        let present: HashSet<CalculationId> = on_disk.iter().map(|file| file.id.clone()).collect();
        let mut seen = HashSet::new();
        let (kept, removed): (Vec<CalculationId>, Vec<CalculationId>) = self
            .order
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .partition(|id| present.contains(id));
        on_disk.retain(|file| !seen.contains(&file.id));
        on_disk.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        let added: Vec<CalculationId> = on_disk.into_iter().map(|file| file.id).collect();
        self.order = added.iter().cloned().chain(kept).collect();
        if self
            .last_open
            .as_ref()
            .is_some_and(|last| !present.contains(last))
        {
            self.last_open = None;
        }
        (added, removed)
    }

    /// Reads every listed file into what the library knows.
    fn remember_all(&mut self) {
        for id in &self.order {
            match files::read(&self.folder, id) {
                Ok(text) => {
                    self.known.insert(id.clone(), text);
                }
                Err(error) => tracing::warn!(%error, %id, "couldn't read a calculation"),
            }
        }
    }

    /// Records `text` as what the library knows of `id`.
    fn remember(&mut self, id: &CalculationId, text: String) {
        self.reported.remove(id);
        self.known.insert(id.clone(), text);
    }

    fn forget(&mut self, id: &CalculationId) {
        self.known.remove(id);
        self.reported.remove(id);
    }

    fn persist(&self) {
        let folder = state::Folder {
            order: self.order.clone(),
            last_open: self.last_open.clone(),
        };
        state::store(&self.state_dir, &self.key, &folder);
    }
}
