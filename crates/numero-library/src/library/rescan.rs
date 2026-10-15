//! Noticing what changed in the folder behind the library's back.

use super::Library;
use crate::Changes;
use crate::files;
use crate::id::CalculationId;

impl Library {
    /// Looks at the folder again. New files go on top of the list and gone
    /// ones leave it; a file whose text differs from what the library last
    /// read or wrote is reported as modified, once per new text, but stays
    /// unaccepted until [`Library::load`]. The library's own writes are
    /// never reported.
    pub fn rescan(&mut self) -> Changes {
        let (added, removed) = self.reconcile();
        for id in &removed {
            self.forget(id);
        }
        for id in &added {
            match files::read(&self.folder, id) {
                Ok(text) => self.remember(id, text),
                Err(error) => tracing::warn!(%error, %id, "couldn't read a new calculation"),
            }
        }
        let unchanged_order: Vec<CalculationId> = self
            .order
            .iter()
            .filter(|id| !added.contains(id))
            .cloned()
            .collect();
        let modified = unchanged_order
            .into_iter()
            .filter(|id| self.changed_on_disk(id))
            .collect();
        if !added.is_empty() || !removed.is_empty() {
            self.persist();
        }
        Changes {
            added,
            removed,
            modified,
        }
    }

    /// Whether the file's text is new: different from what the library
    /// knows and from what it reported last.
    fn changed_on_disk(&mut self, id: &CalculationId) -> bool {
        let on_disk = match files::read(&self.folder, id) {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(%error, %id, "couldn't read a calculation");
                return false;
            }
        };
        match self.known.get(id) {
            // Never read before: this is its text, nothing to report.
            None => {
                self.known.insert(id.clone(), on_disk);
                false
            }
            Some(known) if *known == on_disk => {
                self.reported.remove(id);
                false
            }
            Some(_) if self.reported.get(id) == Some(&on_disk) => false,
            Some(_) => {
                self.reported.insert(id.clone(), on_disk);
                true
            }
        }
    }
}
