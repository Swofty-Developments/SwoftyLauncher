use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::{fnv1a64, fnv1a64_from};

use crate::{Error, Result};

pub fn key(path: &str) -> u64 {
    fnv1a64(path)
}

pub fn at(h: u64, suffix: &str) -> u64 {
    fnv1a64_from(h, suffix)
}

pub fn field(row: u64, name: &str) -> u64 {
    at(row, &format!(".{name}"))
}

pub fn row(table: u64, i: usize) -> u64 {
    at(table, &i.to_string())
}

pub fn sub_table(row: u64, name: &str) -> u64 {
    at(row, &format!(".{name}_"))
}

pub fn sub_count(row: u64, name: &str) -> u64 {
    at(row, &format!("._{name}_instancecount_"))
}

pub struct Db<'a> {
    pub db: &'a mut Gvdb,
    pub added: usize,
    pub changed: usize,
}

impl<'a> Db<'a> {
    pub fn new(db: &'a mut Gvdb) -> Self {
        Self {
            db,
            added: 0,
            changed: 0,
        }
    }

    pub fn get(&self, key: u64) -> Option<&Record> {
        self.db.get_key(key)
    }

    pub fn has(&self, key: u64) -> bool {
        self.db.position(key).is_some()
    }

    pub fn text(&self, key: u64) -> Option<String> {
        self.get(key).map(|r| r.text().into_owned())
    }

    pub fn int(&self, key: u64) -> Option<i64> {
        self.text(key)?.trim().parse().ok()
    }

    pub fn put(&mut self, key: u64, tag: u8, value: &[u8]) {
        match self.db.records.value_mut(key) {
            Some((was, now)) => {
                if now.as_slice() != value || *was != tag {
                    *now = value.to_vec();
                    *was = tag;
                    self.changed += 1;
                }
            }
            None => {
                self.db.records.push(Record {
                    key,
                    tag,
                    value: value.to_vec(),
                });
                self.added += 1;
            }
        }
    }

    pub fn set_text(&mut self, key: u64, value: &str, what: &str) -> Result<()> {
        let tag = self
            .get(key)
            .map(|r| r.tag)
            .ok_or_else(|| Error::Resolve(format!("{what}: no such key")))?;
        self.put(key, tag, value.as_bytes());
        Ok(())
    }

    pub fn copy(&mut self, from: u64, to: u64) -> bool {
        match self.get(from) {
            Some(r) => {
                let (tag, value) = (r.tag, r.value.clone());
                self.put(to, tag, &value);
                true
            }
            None => false,
        }
    }

    pub fn count(&self, key: u64) -> Option<usize> {
        self.int(key).and_then(|n| usize::try_from(n).ok())
    }

    pub fn rows(&self, table: u64, count: u64) -> Vec<u64> {
        (0..self.count(count).unwrap_or(0))
            .map(|i| row(table, i))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_first_record_of_a_key_as_the_database_does() {
        let path = "t.r_0.a";
        let mut gvdb = Gvdb::new(vec![Record::new(path, 4, "first"), Record::new(path, 4, "later")]);
        let mut db = Db::new(&mut gvdb);
        assert_eq!(db.text(fnv1a64(path)).as_deref(), Some("first"));
        db.put(fnv1a64(path), 4, b"put");
        db.put(fnv1a64("t.r_0.b"), 0, b"1");
        assert_eq!((db.changed, db.added), (1, 1));
        assert_eq!(gvdb.get_text(path).as_deref(), Some("put"));
        assert_eq!(gvdb.records[1].text(), "later");
        assert_eq!(gvdb.get_text("t.r_0.b").as_deref(), Some("1"));
    }
}
