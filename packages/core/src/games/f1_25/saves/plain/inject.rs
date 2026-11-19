use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, read_json, write_json};

pub const INJECT: &str = "inject.json";
pub const INJECTED: &str = "injected.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub generation: u64,
    pub container: String,
    pub replace: bool,
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
    #[serde(default)]
    results: Vec<Answer>,
}

pub fn pending(root: &Path) -> Vec<Request> {
    read_json::<Requests>(&root.join(INJECT)).map_or_else(|_| Vec::new(), |r| r.requests)
}

pub fn answers(root: &Path) -> Vec<Answer> {
    read_json::<Answers>(&root.join(INJECTED)).map_or_else(|_| Vec::new(), |a| a.results)
}

pub fn ask(root: &Path, generation: u64, container: &str, replace: bool, now: i64) -> Result<Request> {
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
