//! A bundle the launcher sends by itself when F1 25 crashes in a room: the room the game ran in, as its session
//! folder keeps it, how the send went, and what the bundle says about it.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The file in a session folder that names the room the session's game ran in.
pub const ROOM_FILE: &str = "room.json";
/// The file in a session folder that says how the automatic send for its crash went.
pub const SENT_FILE: &str = "logs-sent.json";

/// The room a game ran in, as the launcher last saw it while the game was running.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomRun {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The session step it was at, by the session module's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default)]
    pub host: bool,
    /// The room started this game.
    #[serde(default)]
    pub launched: bool,
}

impl RoomRun {
    pub fn read(session: &Path) -> Option<Self> {
        crate::read_json(&session.join(ROOM_FILE)).ok()
    }

    /// Writes it into `session`, a session folder that still exists.
    pub fn write(&self, session: &Path) -> crate::Result<()> {
        if !session.is_dir() {
            return Err(crate::Error::Other(format!("{} is gone", session.display())));
        }
        crate::write_json(&session.join(ROOM_FILE), self)
    }

    pub fn forget(session: &Path) {
        std::fs::remove_file(session.join(ROOM_FILE)).ok();
    }

    /// One line for the summary: the code, the step and phase, and who started the game.
    pub fn describe(&self) -> String {
        let mut parts = vec![format!(
            "room {}",
            self.code.as_deref().unwrap_or("(no code yet)")
        )];
        if let Some(step) = &self.step {
            parts.push(format!("step {step}"));
        }
        if let Some(phase) = &self.phase {
            parts.push(format!("phase {phase}"));
        }
        parts.push(if self.host { "hosting" } else { "joined" }.to_owned());
        if self.launched {
            parts.push("the room started the game".to_owned());
        }
        parts.join(", ")
    }
}

/// How the automatic send for a crash went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Outcome {
    Sent {
        parts: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
    },
    Failed {
        why: String,
    },
}

impl Outcome {
    /// What an automatic send that never finished leaves: the launcher closed while it ran.
    pub fn interrupted() -> Self {
        Self::Failed {
            why: "The launcher closed before the send finished".into(),
        }
    }

    pub fn read(session: &Path) -> Option<Self> {
        crate::read_json(&session.join(SENT_FILE)).ok()
    }

    pub fn write(&self, session: &Path) -> crate::Result<()> {
        crate::write_json(&session.join(SENT_FILE), self)
    }
}

/// Why a bundle went without the player asking: the crashed session and the room its game ran in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Automatic {
    /// The crashed session's folder name.
    pub session: String,
    pub room: Option<RoomRun>,
}

impl Automatic {
    /// The message the bundle carries in place of the player's.
    pub fn message(&self) -> String {
        match &self.room {
            Some(room) => format!(
                "Sent automatically after F1 25 crashed in a room ({}). The player wasn't asked to describe it.",
                room.describe()
            ),
            None => "Sent automatically after F1 25 crashed. The player wasn't asked to describe it.".into(),
        }
    }

    /// The summary's lines under the username: how it was sent, the room and the session.
    pub(super) fn summary_lines(&self) -> Vec<(&'static str, String)> {
        let mut lines = vec![(
            "Sent",
            "automatically by the launcher after F1 25 crashed; the player wasn't asked".to_owned(),
        )];
        if let Some(room) = &self.room {
            lines.push(("Room", room.describe()));
        }
        lines.push(("Crashed session", format!("sessions/{}", self.session)));
        lines
    }
}

/// The step the room's state is at, by its `id`: the first failed step, else the last active one; None when its
/// `steps` name neither.
pub fn step_at(room: &Value) -> Option<String> {
    let steps = room.get("steps")?.as_array()?;
    let in_state = |state: &'static str| {
        steps
            .iter()
            .filter(move |s| s.get("state").and_then(Value::as_str) == Some(state))
            .filter_map(|s| s.get("id")?.as_str())
    };
    in_state("failed")
        .next()
        .or_else(|| in_state("active").next_back())
        .map(str::to_owned)
}

/// The step the room's state recorded its game's exit at (`game_exit.step`), when it recorded that exit at or after
/// `since_ms` (Unix milliseconds, `game_exit.at_ms`). A room keeps its last exit after the game comes back, so an
/// exit from before the game in question started is an earlier game's.
pub fn exit_step(room: &Value, since_ms: u64) -> Option<String> {
    let exit = room.get("game_exit")?;
    let at_ms = exit.get("at_ms")?.as_u64()?;
    let step = exit.get("step")?.as_str()?;
    (at_ms >= since_ms).then(|| step.to_owned())
}

/// The room's steps as its state names them (`steps`: each `id`, `state` and a failed one's `problem.code`), in
/// one line: "account done, room done, game failed (game_crashed), network pending".
pub fn steps_line(room: &Value) -> Option<String> {
    let steps: Vec<String> = room
        .get("steps")?
        .as_array()?
        .iter()
        .filter_map(|step| {
            let (id, state) = (step.get("id")?.as_str()?, step.get("state")?.as_str()?);
            Some(match step.pointer("/problem/code").and_then(Value::as_str) {
                Some(code) => format!("{id} {state} ({code})"),
                None => format!("{id} {state}"),
            })
        })
        .collect();
    (!steps.is_empty()).then(|| steps.join(", "))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn session(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-automatic-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn racing() -> RoomRun {
        RoomRun {
            code: Some("K7Q2PX".into()),
            step: Some("in_lobby".into()),
            phase: Some("racing".into()),
            host: false,
            launched: true,
        }
    }

    #[test]
    fn a_room_is_kept_in_its_session_folder_until_forgotten() {
        let dir = session("room");
        assert_eq!(RoomRun::read(&dir), None);
        racing().write(&dir).unwrap();
        assert_eq!(RoomRun::read(&dir), Some(racing()));
        RoomRun::forget(&dir);
        assert_eq!(RoomRun::read(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_room_is_never_written_into_a_session_folder_that_is_gone() {
        let dir = session("gone");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(racing().write(&dir).is_err());
        assert!(!dir.exists());
    }

    #[test]
    fn a_room_describes_where_the_game_was() {
        assert_eq!(
            racing().describe(),
            "room K7Q2PX, step in_lobby, phase racing, joined, the room started the game"
        );
        let joining = RoomRun {
            host: true,
            ..RoomRun::default()
        };
        assert_eq!(joining.describe(), "room (no code yet), hosting");
    }

    #[test]
    fn an_outcome_reads_back_as_written() {
        let dir = session("outcome");
        assert_eq!(Outcome::read(&dir), None);
        let sent = Outcome::Sent {
            parts: 2,
            reference: Some("1311204".into()),
        };
        sent.write(&dir).unwrap();
        assert_eq!(Outcome::read(&dir), Some(sent));
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(SENT_FILE)).unwrap()).unwrap();
        assert_eq!(
            on_disk,
            json!({"result": "sent", "parts": 2, "reference": "1311204"})
        );
        Outcome::interrupted().write(&dir).unwrap();
        assert_eq!(
            Outcome::read(&dir),
            Some(Outcome::Failed {
                why: "The launcher closed before the send finished".into()
            })
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_rooms_steps_read_as_one_line() {
        let room = json!({"steps": [
            {"id": "account", "state": "done"},
            {"id": "game", "state": "failed", "detail": "F1 25 crashed",
             "problem": {"code": "game_crashed", "text": "F1 25 crashed.", "retry": true}},
            {"id": "network", "state": "pending"},
            {"state": "done"},
        ]});
        assert_eq!(
            steps_line(&room).as_deref(),
            Some("account done, game failed (game_crashed), network pending")
        );
        assert_eq!(steps_line(&json!({"phase": "racing"})), None);
        assert_eq!(steps_line(&json!({"steps": []})), None);
    }

    #[test]
    fn the_step_a_room_is_at_is_its_failed_step_else_its_last_active_one() {
        let room = |steps: Value| json!({"phase": {"phase": "racing"}, "steps": steps});
        let racing = room(json!([
            {"id": "account", "state": "done"},
            {"id": "game", "state": "done"},
            {"id": "network", "state": "active"},
            {"id": "lobby", "state": "done"},
            {"id": "race", "state": "active"},
        ]));
        assert_eq!(step_at(&racing).as_deref(), Some("race"));
        let failed = room(json!([
            {"id": "account", "state": "done"},
            {"id": "network", "state": "failed", "problem": {"code": "relay_lost", "text": "The relay went away."}},
            {"id": "lobby", "state": "active"},
            {"id": "race", "state": "failed"},
        ]));
        assert_eq!(step_at(&failed).as_deref(), Some("network"));
        let done = room(json!([
            {"id": "account", "state": "done"},
            {"id": "race", "state": "skipped"},
            {"state": "active"},
        ]));
        assert_eq!(step_at(&done), None);
        assert_eq!(step_at(&json!({"phase": "racing"})), None);
        assert_eq!(step_at(&json!({"steps": "not a list"})), None);
    }

    #[test]
    fn the_exit_step_is_the_one_the_room_recorded_for_this_games_exit() {
        let room = |exit: Value| json!({"steps": [{"id": "race", "state": "active"}], "game_exit": exit});
        let exit = |step: &str, at_ms: u64| {
            json!({"code": "game_crashed", "cause": "crashed", "source": "launcher", "exit_code": -1073741819,
                   "room": "K7Q2PX", "step": step, "at_ms": at_ms})
        };
        assert_eq!(
            exit_step(&room(exit("lobby", 5_000)), 4_000).as_deref(),
            Some("lobby")
        );
        assert_eq!(
            exit_step(&room(exit("lobby", 4_000)), 4_000).as_deref(),
            Some("lobby")
        );
        assert_eq!(
            exit_step(&room(exit("race", 3_999)), 4_000),
            None,
            "an earlier game's exit"
        );
        assert_eq!(exit_step(&json!({"steps": []}), 0), None);
        assert_eq!(
            exit_step(&room(json!({"step": "race"})), 0),
            None,
            "no time, no telling whose exit"
        );
        assert_eq!(exit_step(&room(json!({"at_ms": 5_000})), 0), None);
    }

    #[test]
    fn the_message_names_the_room_or_only_the_crash() {
        let with_room = Automatic {
            session: "20261120-170000".into(),
            room: Some(racing()),
        };
        assert_eq!(
            with_room.message(),
            "Sent automatically after F1 25 crashed in a room (room K7Q2PX, step in_lobby, phase racing, joined, \
             the room started the game). The player wasn't asked to describe it."
        );
        let bare = Automatic {
            session: "20261120-170000".into(),
            room: None,
        };
        assert_eq!(
            bare.message(),
            "Sent automatically after F1 25 crashed. The player wasn't asked to describe it."
        );
    }
}
