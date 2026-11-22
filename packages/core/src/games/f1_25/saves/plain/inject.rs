use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, read_json, write_json};

pub const INJECT: &str = "inject.json";
pub const INJECTED: &str = "injected.json";
/// The requests a profile load took: the core renames `inject.json` to this before it applies any of
/// them and deletes it once `injected.json` holds the answers.
pub const APPLYING: &str = "inject.applying.json";
/// The answer to a request whose profile load stopped before it answered.
pub const WITHDRAWN: &str = "F1 25 stopped while this restore was applied, so the launcher withdrew it; the next \
                             Play loads the save without it";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub generation: u64,
    pub container: String,
    pub replace: bool,
    /// The sha-256 of the container in the generation the rewrite was made from; the core replaces only a
    /// loaded container that hashes the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Requests {
    schema: u32,
    requests: Vec<Request>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Answers {
    #[serde(default = "schema")]
    schema: u32,
    #[serde(default)]
    results: Vec<Answer>,
}

fn schema() -> u32 {
    1
}

pub fn pending(root: &Path) -> Vec<Request> {
    read_json::<Requests>(&root.join(INJECT)).map_or_else(|_| Vec::new(), |r| r.requests)
}

pub fn answers(root: &Path) -> Vec<Answer> {
    read_json::<Answers>(&root.join(INJECTED)).map_or_else(|_| Vec::new(), |a| a.results)
}

/// The requests a profile load took and has not answered yet.
pub fn applying(root: &Path) -> Vec<Request> {
    read_json::<Requests>(&root.join(APPLYING)).map_or_else(|_| Vec::new(), |r| r.requests)
}

/// Answer the requests of a profile load that stopped before it answered them (the game closed or
/// crashed while it applied them) as withdrawn, and remove them so no later load applies them. Only
/// while F1 25 is closed: a running game may still answer them.
pub fn withdraw_unanswered(root: &Path) -> Result<Vec<Request>> {
    let path = root.join(APPLYING);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let requests = applying(root);
    let mut results: Vec<Answer> = answers(root)
        .into_iter()
        .filter(|a| !requests.iter().any(|r| r.id == a.id))
        .collect();
    results.extend(requests.iter().map(|r| Answer {
        id: r.id.clone(),
        ok: false,
        detail: WITHDRAWN.to_owned(),
    }));
    write_json(&root.join(INJECTED), &Answers { schema: 1, results })?;
    std::fs::remove_file(&path).map_err(crate::io_err(&path))?;
    Ok(requests)
}

/// The requests `file` of `root` holds: [`INJECT`] or [`APPLYING`].
pub(super) fn held(root: &Path, file: &str) -> Vec<Request> {
    read_json::<Requests>(&root.join(file)).map_or_else(|_| Vec::new(), |r| r.requests)
}

/// Write `requests` as `file` of `root` whole, or remove the file when there are none.
pub(super) fn hold(root: &Path, file: &str, requests: Vec<Request>) -> Result<()> {
    let path = root.join(file);
    if requests.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(crate::io_err(&path)(e)),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(root).map_err(crate::io_err(root))?;
    write_json(&path, &Requests { schema: 1, requests })
}

/// Add `more` to the answers in [`INJECTED`], each id once.
pub(super) fn answer(root: &Path, more: Vec<Answer>) -> Result<()> {
    let mut results = answers(root);
    for a in more {
        if !results.iter().any(|r| r.id == a.id) {
            results.push(a);
        }
    }
    std::fs::create_dir_all(root).map_err(crate::io_err(root))?;
    write_json(&root.join(INJECTED), &Answers { schema: 1, results })
}

pub fn ask(root: &Path, generation: u64, container: &str, replace: bool, now: i64) -> Result<Request> {
    ask_based(root, generation, container, replace, None, now)
}

pub fn ask_based(
    root: &Path,
    generation: u64,
    container: &str,
    replace: bool,
    base_sha256: Option<String>,
    now: i64,
) -> Result<Request> {
    let mut requests = pending(root);
    if let Some(same) = requests
        .iter()
        .find(|r| r.generation == generation && r.container == container && r.replace == replace)
    {
        return Ok(same.clone());
    }
    requests.retain(|r| r.container != container);
    let request = Request {
        id: format!("r{now}-{container}"),
        generation,
        container: container.to_owned(),
        replace,
        base_sha256,
    };
    requests.push(request.clone());
    std::fs::create_dir_all(root).map_err(crate::io_err(root))?;
    write_json(&root.join(INJECT), &Requests { schema: 1, requests })?;
    Ok(request)
}

pub fn withdraw(root: &Path, id: &str) -> Result<()> {
    let mut requests = pending(root);
    requests.retain(|r| r.id != id);
    match requests.is_empty() {
        true => {
            std::fs::remove_file(root.join(INJECT)).ok();
            Ok(())
        }
        false => write_json(&root.join(INJECT), &Requests { schema: 1, requests }),
    }
}
