//! Creating, saving, loading and deleting calculations.

use std::fs;
use std::io;

use super::Library;
use crate::Saved;
use crate::files;
use crate::id::CalculationId;
use crate::naming::{self, Taken};

impl Library {
    /// Adds a calculation holding `text`, named after its first line, at the
    /// top, and makes it the open one.
    pub fn create(&mut self, text: &str) -> io::Result<CalculationId> {
        let taken = self.taken();
        let id = naming::unique(&naming::stem_for(text), &taken, naming::numbered)?;
        self.write(&id, text)?;
        self.order.insert(0, id.clone());
        self.last_open = Some(id.clone());
        self.persist();
        Ok(id)
    }

    /// Writes `text` to the calculation, unless its file changed on disk
    /// since the library last read or wrote it: then the file is left alone
    /// and `text` goes to a conflict copy, `<name> (conflict).txt` (then
    /// `(conflict 2)`, `(conflict 3)`…), listed right above it and made
    /// the open one. A file gone from disk is written back.
    pub fn save(&mut self, id: &CalculationId, text: &str) -> io::Result<Saved> {
        let on_disk = match files::read(&self.folder, id) {
            Ok(on_disk) => Some(on_disk),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        match on_disk {
            Some(on_disk) if on_disk == text => {
                self.remember(id, on_disk);
                Ok(Saved::Written)
            }
            Some(on_disk) if self.known.get(id).is_some_and(|known| *known != on_disk) => {
                self.save_conflict(id, text)
            }
            Some(_) => {
                self.write(id, text)?;
                Ok(Saved::Written)
            }
            None => {
                self.write(id, text)?;
                if !self.order.contains(id) {
                    self.order.insert(0, id.clone());
                    self.persist();
                }
                Ok(Saved::Written)
            }
        }
    }

    /// The calculation's text on disk, which becomes what the library knows
    /// of it: a change [`Library::rescan`] reported is accepted.
    pub fn load(&mut self, id: &CalculationId) -> io::Result<String> {
        let text = files::read(&self.folder, id)?;
        self.remember(id, text.clone());
        if !self.order.contains(id) {
            self.order.insert(0, id.clone());
            self.persist();
        }
        Ok(text)
    }

    /// Removes the calculation's file and its place in the list.
    pub fn delete(&mut self, id: &CalculationId) -> io::Result<()> {
        match fs::remove_file(self.folder.join(id.as_str())) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.forget(id);
        self.order.retain(|kept| kept != id);
        if self.last_open.as_ref() == Some(id) {
            self.last_open = None;
        }
        self.persist();
        Ok(())
    }

    fn save_conflict(&mut self, id: &CalculationId, text: &str) -> io::Result<Saved> {
        let taken = self.taken();
        let copy = naming::unique(id.stem(), &taken, naming::conflict)?;
        self.write(&copy, text)?;
        tracing::warn!(%id, %copy, "the calculation changed on disk; saved a conflict copy");
        let position = self.order.iter().position(|kept| kept == id).unwrap_or(0);
        self.order.insert(position, copy.clone());
        self.last_open = Some(copy.clone());
        self.persist();
        Ok(Saved::Conflict { copy })
    }

    fn write(&mut self, id: &CalculationId, text: &str) -> io::Result<()> {
        files::write_atomic(&self.folder, id, text)?;
        self.remember(id, text.to_owned());
        Ok(())
    }

    /// Every name in the folder and in the list.
    pub(super) fn taken(&self) -> Taken {
        let names = files::names(&self.folder);
        Taken::new(
            names
                .iter()
                .map(String::as_str)
                .chain(self.order.iter().map(CalculationId::as_str)),
        )
    }
}
