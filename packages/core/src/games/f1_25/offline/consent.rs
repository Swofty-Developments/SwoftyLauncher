use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Result, read_json, write_json, write_json_shared};

use super::{DISCLOSURE, EXTENSION_VERSION};

pub(super) fn consent_path(data: &Path) -> PathBuf {
    data.join("state/offline_consent.json")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consent {
    pub schema: u32,
    pub text_sha256: String,
    pub accepted: bool,
    #[serde(default)]
    pub accepted_utc: String,
    #[serde(default)]
    pub extension_version: String,
}

pub fn consent(data: &Path) -> Option<Consent> {
    read_json(&consent_path(data)).ok()
}

/// True when the terms were accepted and the text has not changed since.
pub fn is_accepted(data: &Path) -> bool {
    consent(data).is_some_and(|c| {
        c.accepted && c.text_sha256 == crate::game_folder::baseline::sha256_bytes(DISCLOSURE.as_bytes())
    })
}

pub fn record_accept(data: &Path) -> Result<Consent> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let c = Consent {
        schema: 1,
        text_sha256: crate::game_folder::baseline::sha256_bytes(DISCLOSURE.as_bytes()),
        accepted: true,
        accepted_utc: secs.to_string(),
        extension_version: EXTENSION_VERSION.into(),
    };
    write_json_shared(&consent_path(data), &c)?;
    Ok(c)
}

pub fn withdraw(data: &Path) -> Result<()> {
    let mut c = consent(data).unwrap_or(Consent {
        schema: 1,
        text_sha256: String::new(),
        accepted: false,
        accepted_utc: String::new(),
        extension_version: String::new(),
    });
    c.accepted = false;
    write_json(&consent_path(data), &c)
}
