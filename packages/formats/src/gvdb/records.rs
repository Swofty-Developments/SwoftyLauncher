//! A database's records in file order, with an index of the first record of
//! each key. The first [`SCANS`] lookups scan the records, and the next one
//! builds the index, which costs about as much as those scans. `push` and
//! `extend` keep the index and `insert` and `remove` shift it; any other change
//! through `&mut` drops it and starts the count of scans again.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Record;

/// Spreads a record key, which is already a hash, over the table's bits.
#[derive(Clone, Copy, Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(b)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type Index = HashMap<u64, usize, BuildHasherDefault<KeyHasher>>;

#[cfg(test)]
thread_local! {
    pub(super) static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Lookups that scan the records before one builds the index.
pub(super) const SCANS: usize = 4;

#[derive(Default)]
pub struct Records {
    list: Vec<Record>,
    index: OnceLock<Index>,
    scans: AtomicUsize,
}

impl Clone for Records {
    fn clone(&self) -> Self {
        Self {
            list: self.list.clone(),
            index: self.index.clone(),
            scans: AtomicUsize::new(self.scans.load(Ordering::Relaxed)),
        }
    }
}

impl Records {
    /// Where the first record of `key` is.
    pub fn position(&self, key: u64) -> Option<usize> {
        if let Some(index) = self.index.get() {
            return index.get(&key).copied();
        }
        if self.scans.fetch_add(1, Ordering::Relaxed) < SCANS {
            return self.list.iter().position(|r| r.key == key);
        }
        self.index().get(&key).copied()
    }

    /// Builds the index now, for a reader about to look up many keys.
    pub fn index_keys(&self) {
        self.index();
    }

    /// The record at `i`, to change its tag or value.
    pub fn at_mut(&mut self, i: usize) -> RecordMut<'_> {
        RecordMut(&mut self.list[i])
    }

    /// The index, when a lookup has built it.
    pub(super) fn built(&self) -> Option<&HashMap<u64, usize, impl std::hash::BuildHasher>> {
        self.index.get()
    }

    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            #[cfg(test)]
            BUILT.with(|n| n.set(n.get() + 1));
            let mut index = Index::with_capacity_and_hasher(self.list.len(), Default::default());
            for (i, r) in self.list.iter().enumerate() {
                index.entry(r.key).or_insert(i);
            }
            index
        })
    }

    /// The first record of `key`, to change its tag or value. Changing its key
    /// leaves the index wrong, so the key is not handed out.
    pub fn value_mut(&mut self, key: u64) -> Option<(&mut u8, &mut Vec<u8>)> {
        let i = self.position(key)?;
        let r = &mut self.list[i];
        Some((&mut r.tag, &mut r.value))
    }

    pub fn push(&mut self, r: Record) {
        if let Some(index) = self.index.get_mut() {
            index.entry(r.key).or_insert(self.list.len());
        }
        self.list.push(r);
    }

    /// Puts `r` at `i`, before the record there.
    pub fn insert(&mut self, i: usize, r: Record) {
        if let Some(index) = self.index.get_mut() {
            for at in index.values_mut() {
                if *at >= i {
                    *at += 1;
                }
            }
            let at = index.entry(r.key).or_insert(i);
            *at = (*at).min(i);
        }
        self.list.insert(i, r);
    }

    pub fn remove(&mut self, i: usize) -> Record {
        let r = self.list.remove(i);
        if let Some(index) = self.index.get_mut() {
            for at in index.values_mut() {
                if *at > i {
                    *at -= 1;
                }
            }
            if index.get(&r.key) == Some(&i) {
                match self.list[i..].iter().position(|o| o.key == r.key) {
                    Some(next) => index.insert(r.key, i + next),
                    None => index.remove(&r.key),
                };
            }
        }
        r
    }

    pub fn into_vec(self) -> Vec<Record> {
        self.list
    }
}

/// A record of [`Records`] lent out to change its tag or value; its key stays.
pub struct RecordMut<'a>(&'a mut Record);

impl RecordMut<'_> {
    pub fn set_tag(&mut self, tag: u8) {
        self.0.tag = tag;
    }

    pub fn set_value(&mut self, value: Vec<u8>) {
        self.0.value = value;
    }

    /// Sets the value and returns the one it had.
    pub fn replace_value(&mut self, value: Vec<u8>) -> Vec<u8> {
        std::mem::replace(&mut self.0.value, value)
    }

    /// Takes the tag and value of `r`, a record of the same key.
    pub fn set(&mut self, r: Record) {
        debug_assert_eq!(r.key, self.0.key);
        (self.0.tag, self.0.value) = (r.tag, r.value);
    }
}

impl Deref for RecordMut<'_> {
    type Target = Record;

    fn deref(&self) -> &Record {
        self.0
    }
}

impl Extend<Record> for Records {
    fn extend<I: IntoIterator<Item = Record>>(&mut self, records: I) {
        for r in records {
            self.push(r);
        }
    }
}

impl Deref for Records {
    type Target = Vec<Record>;

    fn deref(&self) -> &Vec<Record> {
        &self.list
    }
}

impl DerefMut for Records {
    fn deref_mut(&mut self) -> &mut Vec<Record> {
        self.index.take();
        *self.scans.get_mut() = 0;
        &mut self.list
    }
}

impl std::fmt::Debug for Records {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.list.fmt(f)
    }
}

impl PartialEq for Records {
    fn eq(&self, other: &Self) -> bool {
        self.list == other.list
    }
}

impl From<Vec<Record>> for Records {
    fn from(list: Vec<Record>) -> Self {
        Self {
            list,
            index: OnceLock::new(),
            scans: AtomicUsize::new(0),
        }
    }
}

impl FromIterator<Record> for Records {
    fn from_iter<I: IntoIterator<Item = Record>>(records: I) -> Self {
        Vec::from_iter(records).into()
    }
}

impl<'a> IntoIterator for &'a Records {
    type Item = &'a Record;
    type IntoIter = std::slice::Iter<'a, Record>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.iter()
    }
}

impl<'a> IntoIterator for &'a mut Records {
    type Item = &'a mut Record;
    type IntoIter = std::slice::IterMut<'a, Record>;

    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}

impl IntoIterator for Records {
    type Item = Record;
    type IntoIter = std::vec::IntoIter<Record>;

    fn into_iter(self) -> Self::IntoIter {
        self.list.into_iter()
    }
}
