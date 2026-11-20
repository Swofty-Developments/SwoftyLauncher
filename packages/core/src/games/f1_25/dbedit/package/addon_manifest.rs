//! The swofty.json a DB Editor addon writes: the oldest launcher that can load it, and where it came from.

use super::{DEFAULT_AUTHOR, GAME_BUILDS, OPS_FILE, ORIGIN_TOOL, ROWS_LAUNCHER};
use crate::manifest::AddonManifest;
use crate::{Error, Result};

/// The oldest launcher a new addon accepts: the first that reads a manifest's
/// `category` and `origin`, or this one when it is older.
pub(super) fn launcher_floor(v: &semver::Version) -> semver::Version {
    let v = semver::Version {
        pre: v.pre.clone(),
        ..semver::Version::new(v.major, v.minor, v.patch)
    };
    v.min(semver::Version::new(1, 3, 0))
}

/// Raises a `>=` launcher requirement below [`ROWS_LAUNCHER`] to it, or to
/// `current` when this launcher is older (a development build).
pub(super) fn raise_launcher(req: &mut Option<semver::VersionReq>, current: &semver::Version) {
    let current = semver::Version {
        pre: current.pre.clone(),
        ..semver::Version::new(current.major, current.minor, current.patch)
    };
    let floor = ROWS_LAUNCHER.min(current);
    let low = match req.as_ref().map(|r| r.comparators.as_slice()) {
        None => true,
        Some([c]) if c.op == semver::Op::GreaterEq => {
            let v = semver::Version {
                pre: c.pre.clone(),
                ..semver::Version::new(c.major, c.minor.unwrap_or(0), c.patch.unwrap_or(0))
            };
            v < floor
        }
        Some(_) => false,
    };
    if low {
        *req = semver::VersionReq::parse(&format!(">={floor}")).ok();
    }
}

pub(super) fn origin() -> crate::manifest::Origin {
    crate::manifest::Origin {
        tool: ORIGIN_TOOL.into(),
        tool_version: Some(crate::updates::release::current_version().to_string()),
        file: None,
        sha256: None,
    }
}

pub(super) fn manifest(id: &str, name: &str) -> Result<AddonManifest> {
    let launcher = launcher_floor(&crate::updates::release::current_version());
    let m: AddonManifest = serde_json::from_value(serde_json::json!({
        "schema_version": crate::manifest::SCHEMA_VERSION,
        "id": id,
        "name": name,
        "version": "1.0.0",
        "author": DEFAULT_AUTHOR,
        "kind": ["database"],
        "compat": { "game": crate::GAME_ID, "game_builds": [GAME_BUILDS], "launcher": format!(">={launcher}") },
        "operations": { "database": [OPS_FILE] },
    }))
    .map_err(|e| Error::Manifest(e.to_string()))?;
    let m = AddonManifest {
        origin: Some(origin()),
        ..m
    };
    m.validate()?;
    Ok(m)
}
