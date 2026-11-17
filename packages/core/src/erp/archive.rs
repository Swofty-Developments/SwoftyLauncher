use std::borrow::Cow;
use std::path::Path;

use swofty_formats::erp::{ErpFile, Resource};

use super::fail;
use crate::{Error, Result};

pub(super) enum Slot {
    Stock(usize),
    Owned(Resource),
}

pub(super) struct Archive {
    pub(super) file: ErpFile,
    pub(super) slots: Vec<Slot>,
}

impl Archive {
    /// The archive at `path`, every slot the game's own; `rel` names it in errors.
    pub(super) fn open(rel: &str, path: &Path) -> Result<Self> {
        let file = ErpFile::open(path).map_err(|e| fail(format!("{rel}: {e}")))?;
        let slots = (0..file.erp.resources.len()).map(Slot::Stock).collect();
        Ok(Self { file, slots })
    }

    pub(super) fn entry<'s>(&'s self, slot: &'s Slot) -> &'s Resource {
        match slot {
            Slot::Stock(i) => &self.file.erp.resources[*i],
            Slot::Owned(r) => r,
        }
    }

    pub(super) fn name<'s>(&'s self, slot: &'s Slot) -> &'s str {
        &self.entry(slot).name
    }

    pub(super) fn position(&self, name: &str) -> Result<Option<usize>> {
        let mut hits = (0..self.slots.len()).filter(|&i| self.name(&self.slots[i]) == name);
        match (hits.next(), hits.next()) {
            (Some(_), Some(_)) => Err(fail(format!("{name}: more than one resource"))),
            (hit, _) => Ok(hit),
        }
    }

    pub(super) fn find(&self, erp: &str, name: &str) -> Result<usize> {
        self.position(name)
            .map_err(|err| fail(format!("{erp}: {err}")))?
            .ok_or_else(|| Error::ResourceMissing {
                erp: erp.to_owned(),
                resource: name.to_owned(),
            })
    }

    // Read for a stock copy only: every slot still the game's, in its place.
    pub(super) fn untouched(&self) -> bool {
        self.slots.len() == self.file.erp.resources.len()
            && self
                .slots
                .iter()
                .enumerate()
                .all(|(i, s)| matches!(s, Slot::Stock(j) if *j == i))
    }

    /// The resource in slot `i`, read from the game's archive while it is the stock one.
    pub(super) fn loaded(&self, i: usize) -> Result<Cow<'_, Resource>> {
        Ok(match &self.slots[i] {
            Slot::Owned(r) => Cow::Borrowed(r),
            Slot::Stock(j) => Cow::Owned(self.file.load(&self.file.erp.resources[*j])?),
        })
    }

    /// The resources the ops wrote, in slot order.
    pub(super) fn written(&self) -> impl Iterator<Item = &Resource> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Owned(r) => Some(r),
            Slot::Stock(_) => None,
        })
    }

    pub(super) fn written_mut(&mut self) -> impl Iterator<Item = &mut Resource> {
        self.slots.iter_mut().filter_map(|s| match s {
            Slot::Owned(r) => Some(r),
            Slot::Stock(_) => None,
        })
    }

    pub(super) fn owned(&mut self, i: usize) -> Result<&mut Resource> {
        if let Slot::Stock(j) = self.slots[i] {
            self.slots[i] = Slot::Owned(self.file.load(&self.file.erp.resources[j])?);
        }
        match &mut self.slots[i] {
            Slot::Owned(r) => Ok(r),
            Slot::Stock(_) => unreachable!("loaded above"),
        }
    }
}
