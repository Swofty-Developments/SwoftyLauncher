use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::asset::Op;
use crate::dbedit::deps::Copy;
use crate::game_db::entrant_lists::StockLists;

use super::{Planned, Planner, Target};

impl<'a> Planner<'a> {
    /// Gives the rows `add-row` gives an id of its own the ids `ids` keeps; see [`Planner::take_row_ids`].
    pub fn keep_row_ids(&mut self, ids: crate::row_ids::Store) {
        self.row_ids = Some(RefCell::new(ids));
    }

    /// The kept row ids, with the ids the last `apply_db` gave.
    pub fn take_row_ids(&mut self) -> Option<crate::row_ids::Store> {
        self.row_ids.take().map(RefCell::into_inner)
    }

    /// The addon whose write of the record `key` wins, the last in load order.
    pub fn record_writer(&self, key: u64) -> Option<&str> {
        self.writers
            .get(&Target::Record(key))?
            .iter()
            .max_by_key(|w| self.ranks.iter().position(|o| o == *w))
            .map(|w| w.addon.as_str())
    }

    /// The addons whose cars or teams the last `apply_db` left out for want of a free stat id, with how many
    /// of their rows got none.
    pub fn take_id_left_out(&mut self) -> BTreeMap<String, usize> {
        std::mem::take(&mut self.id_left_out)
    }

    /// Lenient addons' changes the last `apply_db` could not apply.
    pub fn take_skipped(&mut self) -> Vec<String> {
        std::mem::take(&mut self.skipped)
    }

    /// Per addon id, the removals of stock records the last `apply_db` left out.
    /// The (addon, other) pairs where `addon` had ops wait for rows that `other` added, so `addon`
    /// has to load below `other`.
    pub fn take_waits(&mut self) -> BTreeSet<(String, String)> {
        std::mem::take(&mut self.waits)
    }

    pub fn take_stock_removals(&mut self) -> BTreeMap<String, usize> {
        std::mem::take(&mut self.stock_removals)
    }

    /// Per addon id, the removals of records the tracks added that the last `apply_db` left out.
    pub fn take_track_removals(&mut self) -> BTreeMap<String, usize> {
        std::mem::take(&mut self.track_removals)
    }

    /// Watches the game's entrant `lists` for an addon's ops that rename or remove one; see [`Planner::take_kept_lists`].
    pub fn keep_entrant_lists(&mut self, lists: StockLists) {
        self.lists = Some(lists);
    }

    /// Per addon id, the entrant lists the last `apply_db` found the addon renaming or removing.
    pub fn take_kept_lists(&mut self) -> BTreeMap<String, Vec<String>> {
        std::mem::take(&mut self.kept_lists)
    }

    /// The `add-row` copies the last `apply_db` made, in load order.
    pub fn take_copies(&mut self) -> Vec<Copy> {
        std::mem::take(&mut self.copies)
    }

    pub fn erp_ops(&self) -> &[Op] {
        &self.erp
    }

    pub fn take_erp(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.erp)
    }

    pub fn take_files(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.files)
    }

    pub fn take_tracks(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.tracks)
    }

    pub fn take_items(&mut self) -> Vec<Op> {
        std::mem::take(&mut self.items)
    }

    pub fn finish(self) -> Planned {
        self.planned
    }
}
