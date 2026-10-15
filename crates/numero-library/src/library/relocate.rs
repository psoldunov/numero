//! Moving every calculation to another folder.

use std::fs;
use std::io;
use std::path::Path;

use super::Library;
use crate::files;
use crate::id::CalculationId;
use crate::naming;
use crate::state;

impl Library {
    /// Moves every calculation into `target` (created if need be) and
    /// returns the library there. Names already used in `target` are never
    /// overwritten: a clash gets `Name 2.txt`. The moved calculations go on
    /// top of any already there, in their order, and the one open last
    /// stays open.
    ///
    /// On an error part way, what moved stays moved and is listed in
    /// `target`, the rest stays put, and the error is returned; open either
    /// folder again to carry on.
    pub fn move_into(mut self, target: &Path) -> io::Result<Library> {
        fs::create_dir_all(target)?;
        if state::key(target) == self.key {
            return Ok(self);
        }
        self.rescan();
        let mut destination = Library::open(target, &self.state_dir);
        let (moved, failure) = self.move_files(target, &destination);

        let last_open = self.last_open.as_ref().and_then(|last| {
            moved
                .iter()
                .find(|(old, _)| old == last)
                .map(|(_, new)| new.clone())
        });
        let arrived = state::Folder {
            order: moved.iter().map(|(_, new)| new.clone()).collect(),
            last_open,
        };
        destination.adopt(&arrived);
        if arrived.last_open.is_some() {
            destination.last_open = arrived.last_open.clone();
        }
        for (old, new) in &moved {
            if let Some(text) = self.known.get(old) {
                destination.remember(new, text.clone());
            }
            self.forget(old);
        }
        destination.persist();
        self.leave(&moved);

        match failure {
            Some(error) => Err(error),
            None => Ok(destination),
        }
    }

    /// Moves the files one by one, stopping at the first error. Returns
    /// each moved file's old and new name, and the error.
    fn move_files(
        &self,
        target: &Path,
        destination: &Library,
    ) -> (Vec<(CalculationId, CalculationId)>, Option<io::Error>) {
        let mut taken = destination.taken();
        let mut moved = Vec::new();
        for id in &self.order {
            let new = match naming::unique(id.stem(), &taken, naming::numbered) {
                Ok(new) => new,
                Err(error) => return (moved, Some(error)),
            };
            match files::move_file(&self.folder.join(id.as_str()), &target.join(new.as_str())) {
                Ok(()) => {
                    taken.insert(&new);
                    moved.push((id.clone(), new));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    tracing::warn!(%id, "a calculation vanished before it could move");
                }
                Err(error) => {
                    tracing::error!(%error, %id, target = %target.display(), "couldn't move a calculation");
                    return (moved, Some(error));
                }
            }
        }
        (moved, None)
    }

    /// Drops the moved calculations from this folder's metadata, and the
    /// folder's entry altogether once nothing is left in it.
    fn leave(&mut self, moved: &[(CalculationId, CalculationId)]) {
        self.order
            .retain(|id| !moved.iter().any(|(old, _)| old == id));
        if self
            .last_open
            .as_ref()
            .is_some_and(|last| !self.order.contains(last))
        {
            self.last_open = None;
        }
        if self.order.is_empty() {
            state::forget(&self.state_dir, &self.key);
        } else {
            self.persist();
        }
    }
}
