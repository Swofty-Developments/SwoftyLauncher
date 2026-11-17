use serde::{Deserialize, Serialize};

/// One line the launcher writes to the Linux helper. Positions and sizes are
/// physical pixels inside the launcher window; `scale` is its scale factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    Show {
        slot: String,
        url: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale: f64,
    },
    Hide {
        slot: String,
    },
    /// Destroys the slot's webview; a later `Show` opens it again.
    Close {
        slot: String,
    },
}

impl Command {
    pub fn line(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_default();
        line.push('\n');
        line
    }

    pub fn parse(line: &str) -> Option<Self> {
        serde_json::from_str(line.trim()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_survives_a_line() {
        let show = Command::Show {
            slot: "video".into(),
            url: "https://ads.swofty.net/app/video?bg=000000".into(),
            x: 10,
            y: -4,
            width: 400,
            height: 225,
            scale: 1.5,
        };
        let hide = Command::Hide { slot: "video".into() };
        let close = Command::Close { slot: "video".into() };
        for command in [show, hide, close] {
            let line = command.line();
            assert!(line.ends_with('\n') && !line[..line.len() - 1].contains('\n'));
            assert_eq!(Command::parse(&line), Some(command));
        }
    }

    #[test]
    fn a_broken_line_is_ignored() {
        assert_eq!(Command::parse("{\"op\":\"explode\"}"), None);
        assert_eq!(Command::parse("not json"), None);
    }
}
