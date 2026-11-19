mod records;

use std::collections::{HashMap, HashSet};

pub use self::records::{RecordMut, Records};
use crate::bytes::Reader;
use crate::hash::fnv1a64;
use crate::{Error, Result};

pub const MASK: &[u8; 30] = b"tG5MYmBZEZZnrH7yg2qTC4qj42Jcq9";
const MASK_START: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    Int,
    Float,
    String,
    Enum,
    Id,
    Bool,
    Time,
    Curve,
    List,
    Vector,
    Other(u8),
}

impl ValueType {
    pub fn from_tag(tag: u8) -> Self {
        match tag {
            0 => Self::Int,
            1 => Self::Float,
            2 => Self::String,
            3 => Self::Enum,
            4 => Self::Id,
            5 => Self::Bool,
            6 => Self::Time,
            9 => Self::Curve,
            13 => Self::List,
            14 => Self::Vector,
            t => Self::Other(t),
        }
    }

    pub fn tag(self) -> u8 {
        match self {
            Self::Int => 0,
            Self::Float => 1,
            Self::String => 2,
            Self::Enum => 3,
            Self::Id => 4,
            Self::Bool => 5,
            Self::Time => 6,
            Self::Curve => 9,
            Self::List => 13,
            Self::Vector => 14,
            Self::Other(t) => t,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: u64,
    pub tag: u8,
    pub value: Vec<u8>,
}

impl Record {
    pub fn new(path: &str, tag: u8, value: impl Into<Vec<u8>>) -> Self {
        Self {
            key: fnv1a64(path),
            tag,
            value: value.into(),
        }
    }

    pub fn key_hi(&self) -> u32 {
        (self.key >> 32) as u32
    }

    pub fn key_lo(&self) -> u32 {
        self.key as u32
    }

    pub fn value_type(&self) -> ValueType {
        ValueType::from_tag(self.tag)
    }

    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.value)
    }
}

fn transform<const ENCODE: bool>(bytes: &mut [u8], stream: usize) {
    let mut offset = (MASK_START + stream) % MASK.len();
    let mut rest = bytes;
    while !rest.is_empty() {
        let n = rest.len().min(MASK.len() - offset);
        let (chunk, tail) = rest.split_at_mut(n);
        for (b, &mask) in chunk.iter_mut().zip(&MASK[offset..offset + n]) {
            *b = if ENCODE {
                b.wrapping_add(mask)
            } else {
                b.wrapping_sub(mask)
            };
        }
        rest = tail;
        offset = 0;
    }
}

pub fn unmask(enc: &[u8], stream: usize) -> Vec<u8> {
    let mut out = enc.to_vec();
    transform::<false>(&mut out, stream);
    out
}

pub fn mask(plain: &[u8], stream: usize) -> Vec<u8> {
    let mut out = plain.to_vec();
    transform::<true>(&mut out, stream);
    out
}

pub fn instance_count_path(container: &str, row: &str) -> String {
    format!("{container}._{row}_instancecount_")
}

#[derive(Debug, Clone, Default)]
pub struct Gvdb {
    pub records: Records,
}

impl From<Vec<Record>> for Gvdb {
    fn from(records: Vec<Record>) -> Self {
        Self::new(records)
    }
}

impl Gvdb {
    pub fn new(records: Vec<Record>) -> Self {
        Self {
            records: records.into(),
        }
    }

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, 0);
        let count = r.u32()? as usize;
        // A record takes at least 14 bytes, so reserve no more than the data can hold:
        // a foreign file's count is garbage and would otherwise reserve gigabytes.
        let mut records = Vec::with_capacity(count.min(data.len() / 14));
        let mut stream = 0;
        for _ in 0..count {
            let lo = r.u32()?;
            let hi = r.u32()?;
            let tag = r.u8()?;
            let n = r.u32()? as usize;
            let enc = r.take(n)?;
            if r.u8()? != 0 {
                return Err(Error::Format(format!("missing terminator at {:#x}", r.pos - 1)));
            }
            records.push(Record {
                key: (hi as u64) << 32 | lo as u64,
                tag,
                value: unmask(enc, stream),
            });
            stream += n;
        }
        if r.pos != data.len() {
            return Err(Error::Format(format!(
                "trailing bytes: parsed {} of {}",
                r.pos,
                data.len()
            )));
        }
        Ok(Self::new(records))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut order: Vec<&Record> = self.records.iter().collect();
        order.sort_by_key(|r| r.key_hi());
        write_records(order.iter().copied())
    }

    pub fn to_bytes_unsorted(&self) -> Vec<u8> {
        write_records(self.records.iter())
    }

    /// Where the first record of `key` is.
    pub fn position(&self, key: u64) -> Option<usize> {
        self.records.position(key)
    }

    /// The first record of `key`.
    pub fn get_key(&self, key: u64) -> Option<&Record> {
        self.position(key).map(|i| &self.records[i])
    }

    pub fn get(&self, path: &str) -> Option<&Record> {
        self.get_key(fnv1a64(path))
    }

    /// The record [`Gvdb::get_key`] finds for each of `keys` the database
    /// holds: from the index when a lookup has built it, else in one pass
    /// over the records, which costs less than building it.
    pub fn get_keys(&self, keys: &HashSet<u64>) -> HashMap<u64, &Record> {
        if let Some(index) = self.records.built() {
            return keys
                .iter()
                .filter_map(|k| Some((*k, &self.records[*index.get(k)?])))
                .collect();
        }
        let mut out = HashMap::with_capacity(keys.len());
        for r in &self.records {
            if out.len() == keys.len() {
                break;
            }
            if keys.contains(&r.key) {
                out.entry(r.key).or_insert(r);
            }
        }
        out
    }

    pub fn get_text(&self, path: &str) -> Option<String> {
        self.get(path).map(|r| r.text().into_owned())
    }

    pub fn set(&mut self, path: &str, value: impl Into<Vec<u8>>, tag: Option<u8>) -> Result<bool> {
        let key = fnv1a64(path);
        let value = value.into();
        if let Some((was, old)) = self.records.value_mut(key) {
            *old = value;
            if let Some(t) = tag {
                *was = t;
            }
            return Ok(true);
        }
        let tag = tag.ok_or_else(|| Error::Format(format!("new key {path} needs a type tag")))?;
        self.records.push(Record { key, tag, value });
        Ok(false)
    }

    pub fn remove(&mut self, path: &str) -> Option<Record> {
        let i = self.position(fnv1a64(path))?;
        Some(self.records.remove(i))
    }

    pub fn instance_count(&self, container: &str, row: &str) -> Option<usize> {
        self.get_text(&instance_count_path(container, row))?
            .trim()
            .parse()
            .ok()
    }
}

fn write_records<'a>(records: impl ExactSizeIterator<Item = &'a Record> + Clone) -> Vec<u8> {
    let size = 4 + records.clone().map(|r| 14 + r.value.len()).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&(records.len() as u32).to_le_bytes());
    let mut stream = 0;
    for r in records {
        out.extend_from_slice(&r.key_lo().to_le_bytes());
        out.extend_from_slice(&r.key_hi().to_le_bytes());
        out.push(r.tag);
        out.extend_from_slice(&(r.value.len() as u32).to_le_bytes());
        let start = out.len();
        out.extend_from_slice(&r.value);
        transform::<true>(&mut out[start..], stream);
        out.push(0);
        stream += r.value.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Gvdb {
        Gvdb::new(vec![
            Record::new("tracks.Track_3.instanceName", 4, "bahrain"),
            Record::new("tracks.Track_3.numberOfCorners", 0, "15"),
            Record::new("tracks.Track_3.allow_night_start", 5, "True"),
            Record::new("tracks._Track_instancecount_", 0, "43"),
        ])
    }

    #[test]
    fn mask_round_trip() {
        let plain = b"Launched with Swofty Launcher";
        for pos in [0, 1, 29, 30, 12345] {
            assert_eq!(unmask(&mask(plain, pos), pos), plain);
        }
    }

    #[test]
    fn write_then_parse() {
        let db = sample();
        let bytes = db.to_bytes();
        let back = Gvdb::parse(&bytes).unwrap();
        assert_eq!(back.to_bytes(), bytes);
        assert_eq!(
            back.get_text("tracks.Track_3.instanceName").as_deref(),
            Some("bahrain")
        );
        assert_eq!(back.instance_count("tracks", "Track"), Some(43));
    }

    #[test]
    fn sorts_by_key_hi_stably() {
        let bytes = sample().to_bytes();
        let back = Gvdb::parse(&bytes).unwrap();
        let his: Vec<u32> = back.records.iter().map(Record::key_hi).collect();
        assert!(his.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn set_existing_and_new() {
        let mut db = sample();
        assert!(db.set("tracks.Track_3.numberOfCorners", "16", None).unwrap());
        assert!(db.set("tracks.Track_3.new_field", "x", None).is_err());
        assert!(!db.set("tracks.Track_3.new_field", "x", Some(2)).unwrap());
        assert_eq!(
            db.get_text("tracks.Track_3.numberOfCorners").as_deref(),
            Some("16")
        );
    }

    fn built() -> usize {
        records::BUILT.with(|n| n.get())
    }

    fn index(db: &Gvdb) {
        for _ in 0..=records::SCANS {
            db.get_key(0);
        }
    }

    #[test]
    fn a_few_lookups_scan_and_the_next_builds_an_index_once() {
        let mut db = sample();
        db.records
            .push(Record::new("tracks.Track_3.instanceName", 4, "sakhir"));
        let start = built();
        for _ in 0..records::SCANS {
            db.get_key(0);
        }
        assert_eq!(built() - start, 0);
        for _ in 0..3 {
            assert_eq!(
                db.get_text("tracks.Track_3.instanceName").as_deref(),
                Some("bahrain")
            );
            assert_eq!(db.instance_count("tracks", "Track"), Some(43));
            assert_eq!(db.get("tracks.Track_9.instanceName"), None);
        }
        assert_eq!(
            db.clone().get_text("tracks.Track_3.instanceName").as_deref(),
            Some("bahrain")
        );
        assert_eq!(built() - start, 1);
    }

    #[test]
    fn pushing_and_setting_keep_the_index_and_any_other_change_rebuilds_it() {
        let mut db = sample();
        let start = built();
        index(&db);
        assert_eq!(db.position(fnv1a64("tracks._Track_instancecount_")), Some(3));
        db.records
            .push(Record::new("tracks.Track_4.instanceName", 4, "imola"));
        db.records
            .push(Record::new("tracks.Track_4.instanceName", 4, "later"));
        db.records
            .extend([Record::new("tracks.Track_5.instanceName", 4, "monza")]);
        assert!(db.set("tracks.Track_3.numberOfCorners", "16", None).unwrap());
        assert!(!db.set("tracks.Track_6.instanceName", "baku", Some(4)).unwrap());
        let text = |db: &Gvdb, path: &str| db.get_text(path);
        assert_eq!(text(&db, "tracks.Track_4.instanceName").as_deref(), Some("imola"));
        assert_eq!(text(&db, "tracks.Track_5.instanceName").as_deref(), Some("monza"));
        assert_eq!(text(&db, "tracks.Track_6.instanceName").as_deref(), Some("baku"));
        assert_eq!(text(&db, "tracks.Track_3.numberOfCorners").as_deref(), Some("16"));
        assert_eq!(built() - start, 1);

        db.records.iter_mut().find(|r| r.text() == "bahrain").unwrap().key =
            fnv1a64("tracks.Track_7.instanceName");
        index(&db);
        assert_eq!(
            text(&db, "tracks.Track_7.instanceName").as_deref(),
            Some("bahrain")
        );
        assert_eq!(db.get("tracks.Track_3.instanceName"), None);
        db.records.retain(|r| r.text() != "imola");
        index(&db);
        assert_eq!(text(&db, "tracks.Track_4.instanceName").as_deref(), Some("later"));
        db.records.sort_by_key(|r| r.key);
        index(&db);
        for r in db.records.iter() {
            assert_eq!(db.get_key(r.key), Some(r));
        }
        for r in &mut db.records {
            r.key ^= 1;
        }
        assert_eq!(db.get("tracks.Track_5.instanceName"), None);
        index(&db);
        assert_eq!(db.get("tracks.Track_5.instanceName"), None);
        assert_eq!(built() - start, 5);
    }

    #[test]
    fn removing_a_record_shifts_the_index_and_a_later_record_of_its_key_takes_its_place() {
        let mut db = sample();
        db.records
            .push(Record::new("tracks.Track_3.instanceName", 4, "sakhir"));
        let start = built();
        index(&db);
        let count = fnv1a64("tracks._Track_instancecount_");
        let name = "tracks.Track_3.instanceName";
        assert_eq!(db.get_text(name).as_deref(), Some("bahrain"));
        assert_eq!(
            db.remove(name).map(|r| r.text().into_owned()).as_deref(),
            Some("bahrain")
        );
        assert_eq!(db.get_text(name).as_deref(), Some("sakhir"));
        assert_eq!(db.position(count), Some(2));
        assert!(db.remove("tracks.Track_3.numberOfCorners").is_some());
        assert_eq!(db.position(count), Some(1));
        assert_eq!(db.position(fnv1a64(name)), Some(2));
        assert!(db.remove(name).is_some());
        assert_eq!(db.get(name), None);
        assert_eq!(db.records.len(), 2);
        assert_eq!(built() - start, 1);
    }

    #[test]
    fn inserting_a_record_shifts_the_index_and_the_first_record_of_its_key_stays_first() {
        let mut db = sample();
        let name = "tracks.Track_3.instanceName";
        let start = built();
        index(&db);
        db.records.insert(1, Record::new(name, 4, "sakhir"));
        assert_eq!(db.get_text(name).as_deref(), Some("bahrain"));
        db.records.insert(0, Record::new(name, 4, "first"));
        assert_eq!(db.get_text(name).as_deref(), Some("first"));
        assert_eq!(db.position(fnv1a64("tracks._Track_instancecount_")), Some(5));
        db.records
            .insert(6, Record::new("tracks.Track_4.instanceName", 4, "imola"));
        assert_eq!(db.position(fnv1a64("tracks.Track_4.instanceName")), Some(6));
        for (i, r) in db.records.iter().enumerate() {
            assert!(db.position(r.key).is_some_and(|at| at <= i));
        }
        assert_eq!(built() - start, 1);
    }

    #[test]
    fn a_record_lent_out_by_place_changes_its_tag_and_value_and_keeps_the_index() {
        let mut db = sample();
        let start = built();
        db.records.index_keys();
        db.records.index_keys();
        assert_eq!(built() - start, 1);
        let corners = "tracks.Track_3.numberOfCorners";
        let i = db.position(fnv1a64(corners)).unwrap();
        let mut r = db.records.at_mut(i);
        assert_eq!(r.replace_value(b"16".to_vec()), b"15");
        r.set_tag(2);
        db.records
            .at_mut(0)
            .set(Record::new("tracks.Track_3.instanceName", 4, "sakhir"));
        assert_eq!(db.get_text(corners).as_deref(), Some("16"));
        assert_eq!(db.get(corners).map(|r| r.tag), Some(2));
        assert_eq!(
            db.get_text("tracks.Track_3.instanceName").as_deref(),
            Some("sakhir")
        );
        assert_eq!(built() - start, 1);
    }

    #[test]
    fn many_keys_come_from_the_index_once_one_is_built() {
        let mut db = sample();
        db.records
            .push(Record::new("tracks.Track_3.instanceName", 4, "sakhir"));
        let paths = ["tracks.Track_3.instanceName", "tracks.Track_9.instanceName"];
        let keys = paths.iter().map(fnv1a64).collect();
        let start = built();
        let scanned: Vec<_> = db.get_keys(&keys).into_iter().collect();
        assert_eq!(built() - start, 0);
        index(&db);
        let indexed: Vec<_> = db.get_keys(&keys).into_iter().collect();
        assert_eq!(scanned, indexed);
        assert_eq!(indexed, [(fnv1a64(paths[0]), &db.records[0])]);
    }

    #[test]
    fn finds_many_keys_in_one_pass_as_one_lookup_each_finds_them() {
        let mut db = sample();
        db.records
            .push(Record::new("tracks.Track_3.instanceName", 4, "sakhir"));
        let paths = [
            "tracks.Track_3.instanceName",
            "tracks._Track_instancecount_",
            "tracks.Track_9.instanceName",
        ];
        let found = db.get_keys(&paths.iter().map(fnv1a64).collect());
        assert_eq!(found.len(), 2);
        for path in paths {
            assert_eq!(found.get(&fnv1a64(path)).copied(), db.get(path));
        }
        assert_eq!(found[&fnv1a64(paths[0])].text(), "bahrain");
        assert!(db.get_keys(&HashSet::new()).is_empty());
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = sample().to_bytes();
        bytes.push(0);
        assert!(Gvdb::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_a_count_the_data_cannot_hold_without_reserving_it() {
        let mut bytes = u32::MAX.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0; 64]);
        assert!(Gvdb::parse(&bytes).is_err());
    }
}
