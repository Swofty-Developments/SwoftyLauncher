//! A build hook's reply kept for the next Play, which is answered with it, without starting the helper,
//! while every path the reply's `reads` names, every answer the launcher gave the helper's host calls, the
//! helper's own folder and data folder, the build params and the hello are all unchanged.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use swofty_addon::helper::{Built, Call, Hello, HostCall};
use swofty_addon::launcher::Cx;

use super::host_calls::host_call;
use crate::build::digests::{Times, times};

const VERSION: u32 = 1;

/// A path changed this recently may be written again within the same tick of the file system's clock,
/// leaving its size and times as they were, so a reply that read it is not kept.
pub(super) const SETTLED: Duration = Duration::from_secs(2);

/// A host call the helper sent during the build, and the launcher's answer to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Asked {
    pub call: HostCall,
    pub answer: Result<Value, String>,
}

/// Whether a hook's build ran or its kept reply answered it, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookRun {
    Reused(String),
    Ran(String),
}

impl std::fmt::Display for HookRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reused(why) => write!(f, "reused: {why}"),
            Self::Ran(why) => write!(f, "ran: {why}"),
        }
    }
}

/// A file's size and times, a folder's every name and each file's size and times in it, or a path that
/// does not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stamp {
    Missing,
    File { size: u64, times: Times },
    Folder(Vec<(String, Option<(u64, Times)>)>),
}

impl Stamp {
    fn newest(&self) -> Option<i128> {
        let of = |t: &Times| t.modified.max(t.changed);
        match self {
            Self::Missing => None,
            Self::File { times, .. } => Some(of(times)),
            Self::Folder(entries) => entries
                .iter()
                .filter_map(|(_, f)| f.as_ref().map(|(_, t)| of(t)))
                .max(),
        }
    }
}

/// The hello and build params a reply was sent for.
pub(super) struct Asking {
    pub hello: Value,
    pub params: Value,
}

impl Asking {
    pub fn new(hello: &Hello, params: &Call) -> Self {
        Self {
            hello: json(hello),
            params: json(params),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Kept {
    version: u32,
    hello: Value,
    params: Value,
    outcome: Outcome,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    /// Why the reply cannot answer a later build.
    Refused(String),
    Reply {
        reply: Box<Built>,
        asked: Vec<Asked>,
        own: Stamp,
        data: Stamp,
        reads: Vec<(PathBuf, Stamp)>,
    },
}

/// The paths a build hook depends on besides its reads: its own folder and its data folder.
pub(super) struct Folders<'a> {
    pub own: &'a Path,
    pub data: &'a Path,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Stamp::Missing),
        Err(_) => return None,
    };
    if !meta.is_dir() {
        return Some(Stamp::File {
            size: meta.len(),
            times: times(path, &meta)?,
        });
    }
    let mut entries = Vec::new();
    walk(path, "", &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Some(Stamp::Folder(entries))
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, Option<(u64, Times)>)>) -> Option<()> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().into_string().ok()?;
        let rel = match rel {
            "" => name,
            _ => format!("{rel}/{name}"),
        };
        let path = entry.path();
        if entry.file_type().ok()?.is_dir() {
            out.push((rel.clone(), None));
            walk(&path, &rel, out)?;
        } else {
            let meta = std::fs::metadata(&path).ok()?;
            out.push((rel, Some((meta.len(), times(&path, &meta)?))));
        }
    }
    Some(())
}

fn method(call: &HostCall) -> String {
    serde_json::to_value(call)
        .ok()
        .and_then(|v| v["method"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn replayable(call: &HostCall) -> bool {
    !matches!(
        call,
        HostCall::SetState(_)
            | HostCall::InstallAddon { .. }
            | HostCall::SetAddonEnabled { .. }
            | HostCall::RemoveAddon { .. }
            | HostCall::MoveAddon { .. }
    )
}

fn json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// The first field of `hello` that differs from the kept one's.
fn changed_field(was: &Value, now: &Value) -> String {
    let (Some(was), Some(now)) = (was.as_object(), now.as_object()) else {
        return "hello".into();
    };
    was.keys()
        .chain(now.keys())
        .find(|k| was.get(*k) != now.get(*k))
        .map_or_else(|| "hello".into(), |k| k.to_owned())
}

/// The reply kept at `at` and why it may answer this build, or why the build runs.
pub(super) fn reuse(
    at: &Path,
    asking: &Asking,
    folders: &Folders,
    cx: &Cx,
) -> std::result::Result<(Built, String), String> {
    let none = "no reply is kept from an earlier build";
    let kept: Kept = std::fs::read(at)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .filter(|k: &Kept| k.version == VERSION)
        .ok_or(none)?;
    let (reply, asked, own, data, reads) = match kept.outcome {
        Outcome::Refused(why) => return Err(why),
        Outcome::Reply {
            reply,
            asked,
            own,
            data,
            reads,
        } => (*reply, asked, own, data, reads),
    };
    if kept.hello != asking.hello {
        return Err(format!(
            "the hello's {} changed",
            changed_field(&kept.hello, &asking.hello)
        ));
    }
    if kept.params != asking.params {
        return Err("its options or the launcher's features changed".into());
    }
    if stamp(folders.own).as_ref() != Some(&own) {
        return Err("its own folder changed".into());
    }
    if stamp(folders.data).as_ref() != Some(&data) {
        return Err("its data folder changed".into());
    }
    for (path, was) in &reads {
        if stamp(path).as_ref() != Some(was) {
            return Err(format!("{} changed", path.display()));
        }
    }
    for a in &asked {
        if host_call(cx, a.call.clone()) != a.answer {
            return Err(format!("the launcher's answer to {} changed", method(&a.call)));
        }
    }
    Ok((
        reply,
        format!(
            "{} read path(s), {} host call answer(s) and its own and data folders are unchanged",
            reads.len(),
            asked.len()
        ),
    ))
}

/// What `built`, sent by a build that started at `started` and asked `asked`, depends on, or why it
/// cannot answer a later build.
fn outcome(
    built: &Built,
    asked: Vec<Asked>,
    started: SystemTime,
    folders: &Folders,
    settled: Duration,
) -> Outcome {
    let Some(reads) = &built.reads else {
        return Outcome::Refused("its last reply listed no reads".into());
    };
    if let Some(a) = asked.iter().find(|a| !replayable(&a.call)) {
        return Outcome::Refused(format!("its last build sent {}", method(&a.call)));
    }
    if let Some(p) = reads.iter().find(|p| !p.is_absolute()) {
        return Outcome::Refused(format!(
            "its last reply's reads name {}, which is not an absolute path",
            p.display()
        ));
    }
    let since = started
        .checked_sub(settled)
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i128);
    let mut stamps = Vec::new();
    for path in [folders.own, folders.data]
        .into_iter()
        .chain(reads.iter().map(PathBuf::as_path))
    {
        match stamp(path) {
            None => {
                return Outcome::Refused(format!(
                    "{} could not be read when its last build ended",
                    path.display()
                ));
            }
            Some(s) if s.newest().is_some_and(|t| t > since) => {
                return Outcome::Refused(format!(
                    "{} changed less than {} s before its last build ended",
                    path.display(),
                    settled.as_secs()
                ));
            }
            Some(s) => stamps.push(s),
        }
    }
    let mut stamps = stamps.into_iter();
    let (Some(own), Some(data)) = (stamps.next(), stamps.next()) else {
        unreachable!("both folders are stamped first");
    };
    Outcome::Reply {
        reply: Box::new(built.clone()),
        asked,
        own,
        data,
        reads: reads.iter().cloned().zip(stamps).collect(),
    }
}

/// Keeps `built` at `at` for the next build, or, when it cannot answer one, why not.
pub(super) fn keep(
    at: &Path,
    asking: Asking,
    built: &Built,
    asked: Vec<Asked>,
    started: SystemTime,
    folders: &Folders,
    settled: Duration,
) {
    let kept = Kept {
        version: VERSION,
        hello: asking.hello,
        params: asking.params,
        outcome: outcome(built, asked, started, folders, settled),
    };
    if crate::write_json_shared(at, &kept).is_err() {
        forget(at);
    }
}

pub(super) fn forget(at: &Path) {
    std::fs::remove_file(at).ok();
}
