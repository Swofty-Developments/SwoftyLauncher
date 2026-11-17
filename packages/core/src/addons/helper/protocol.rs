//! The JSON-lines conversation with a running helper program.

use std::io::Write;
use std::process::{Child, ChildStdin};
use std::sync::mpsc::Receiver;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;
use swofty_addon::launcher::ProfileInfo;

use super::Result;

pub(super) enum Run {
    Idle,
    Live(Live),
    Dead(String),
}

pub(super) struct Live {
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) lines: Receiver<String>,
    pub(super) next: u64,
    pub(super) profile: ProfileInfo,
}

#[derive(Deserialize)]
pub(super) struct RawReply<'a> {
    id: u64,
    #[serde(borrow)]
    result: &'a RawValue,
}

/// Keep a successful helper payload inside its original input line. Parsing an
/// untagged Incoming/Reply into Value first creates a large temporary tree for
/// chart samples. Only control messages need the existing dynamic decoder.
pub(super) struct JsonResult {
    pub(super) line: String,
    pub(super) range: std::ops::Range<usize>,
}
impl JsonResult {
    pub(super) fn get(&self) -> &str {
        &self.line[self.range.clone()]
    }
}

pub(super) fn reply_range(line: &str) -> Option<(u64, std::ops::Range<usize>)> {
    let reply: RawReply<'_> = serde_json::from_str(line).ok()?;
    let start = reply.result.get().as_ptr() as usize - line.as_ptr() as usize;
    Some((reply.id, start..start + reply.result.get().len()))
}

impl Drop for Live {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

pub(super) fn to_value(v: impl Serialize) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| swofty_addon::Error::Other(e.to_string()))
}

pub(super) fn send(live: &mut Live, message: &impl Serialize) -> std::result::Result<(), String> {
    let mut line = serde_json::to_vec(message).map_err(|e| e.to_string())?;
    line.push(b'\n');
    live.stdin
        .write_all(&line)
        .and_then(|()| live.stdin.flush())
        .map_err(|e| format!("stopped reading ({e})"))
}

#[cfg(test)]
mod tests {
    use super::reply_range;

    #[test]
    fn reply_payload_borrows_original_bytes_in_any_field_order() {
        let line = r#" {"before":"é", "result":{"points":[{"x":1,"y":2},null],"text":"日本語"}, "id":19} "#;
        let (id, range) = reply_range(line).unwrap();
        assert_eq!(id, 19);
        assert_eq!(&line[range], r#"{"points":[{"x":1,"y":2},null],"text":"日本語"}"#);
    }

    #[test]
    fn null_result_is_a_valid_reply_and_control_errors_stay_on_existing_decoder() {
        let (id, range) = reply_range(r#"{"id":0,"result":null}"#).unwrap();
        assert_eq!(id, 0);
        assert_eq!(&r#"{"id":0,"result":null}"#[range], "null");
        for message in [
            r#"{"id":1,"error":"nope"}"#,
            r#"{"id":3,"method":"get_state"}"#,
            r#"{"method":"invalidate"}"#,
            r#"{"id":1,"result":[]"#,
            r#"{"id":1,"result":[]} trailing"#,
        ] {
            assert!(reply_range(message).is_none(), "{message}");
        }
    }
}
