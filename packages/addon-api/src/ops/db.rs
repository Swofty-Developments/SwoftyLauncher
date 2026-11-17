use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DbOps {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strict: bool,
    #[serde(default, skip_serializing_if = "SideData::is_empty")]
    pub side_data: SideData,
    pub ops: Vec<DbOp>,
}

/// The file's settings for the build's side-data check of its `add-row` copies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SideData {
    /// Tables and fields whose side-data findings were reviewed: dotted paths
    /// with or without row numbers, `*` matching any run of characters.
    #[serde(default, deserialize_with = "patterns", skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<String>,
}

impl SideData {
    pub fn is_empty(&self) -> bool {
        self.ignore.is_empty()
    }
}

fn patterns<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    let v = Vec::<String>::deserialize(d)?;
    for p in &v {
        let ok = !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.*{}[]=-".contains(&b));
        if !ok {
            return Err(serde::de::Error::custom(format!(
                "side_data.ignore: {p:?} is not a table or field path"
            )));
        }
    }
    Ok(v)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CopyMode {
    All,
    Listed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StatIdMode {
    Fresh,
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum DbOp {
    Set {
        select: String,
        field: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_from: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        datasets: Vec<String>,
        #[serde(default)]
        create: bool,
        #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
        tag: Option<u8>,
    },
    AddRow {
        table: String,
        from: String,
        #[serde(default)]
        set: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        copy: Option<CopyMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stat_id: Option<StatIdMode>,
    },
    RemoveRow {
        select: String,
    },
    ListAppend {
        select: String,
        field: String,
        value: String,
    },
    ListRemove {
        select: String,
        field: String,
        value: String,
    },
    PutRecord {
        key: String,
        #[serde(rename = "type")]
        tag: u8,
        value: String,
    },
    /// Removing a key already absent is a no-op. A stock record stays unless
    /// `allow_stock_removal` is set, and a record a track added always stays;
    /// see [`FieldNames::stock`] and [`FieldNames::track`].
    RemoveRecord {
        key: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        allow_stock_removal: bool,
    },
    /// Marks the entrant list labelled `list` as virtual: the build moves its row above the table's count, so the
    /// game loads no list object for it and it takes no stat id and no entrant setup mode slot. The core reads it
    /// into `swofty_virtual_list` when a box naming it is pressed. Put it before the `add-row` that adds the list:
    /// that row then passes the 255 ceiling of `entrants.entrant_lists`.
    VirtualList {
        list: String,
    },
}

/// How a row got the id the install keeps for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdFrom {
    /// The profile's last build before the store gave its row that id.
    Seeded,
    /// The hash of its key, or the next id after it that was free (1.6.10 and 1.6.11 builds).
    Hashed,
    /// The next free id above the table's rows in load order, as builds before 1.6.10 gave it.
    Next,
    /// The kept id was taken, and the row took a new one.
    Moved,
    /// The table was full, and the row took an id kept for an addon the build left out.
    Reused,
}
