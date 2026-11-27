use std::io::{IsTerminal, Write};
use std::sync::Mutex;

struct Tee {
    file: std::fs::File,
    left: u64,
}

static TEE: Mutex<Option<Tee>> = Mutex::new(None);

/// Also append every line said from now on to `file`, up to `budget` bytes.
pub(crate) fn tee_to(file: std::fs::File, budget: u64) {
    if let Ok(mut tee) = TEE.lock() {
        *tee = Some(Tee { file, left: budget });
    }
}

/// A line for the tee file only; the console has it already.
pub(crate) fn tee_line(kind: &str, text: &str) {
    let Ok(mut guard) = TEE.lock() else { return };
    let Some(tee) = guard.as_mut() else { return };
    let mut line = Vec::new();
    write_line(&mut line, kind, text, false);
    let len = line.len() as u64;
    if len <= tee.left && tee.file.write_all(&line).is_ok() {
        tee.left -= len;
    } else {
        tee.left = 0;
    }
}

pub(crate) fn info(line: &str) {
    say("info", line);
}

pub(crate) fn say(kind: &str, text: &str) {
    let out = std::io::stdout();
    let colour = out.is_terminal();
    write_line(&mut out.lock(), kind, text, colour);
    tee_line(kind, text);
}

pub(crate) fn blank() {
    writeln!(std::io::stdout().lock()).ok();
}

fn write_line(out: &mut impl Write, kind: &str, text: &str, colour: bool) {
    let code = match kind {
        "loader" => "36",
        "core" => "32",
        "wine" => "33",
        "crash" => "31;1",
        _ => "90",
    };
    let written = if colour {
        writeln!(out, "\x1b[{code}m[{kind}]\x1b[0m {text}")
    } else {
        writeln!(out, "[{kind}] {text}")
    };
    written.ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Full;

    impl Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(28))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from_raw_os_error(28))
        }
    }

    #[test]
    fn a_line_to_a_full_disk_is_dropped_and_the_session_keeps_running() {
        write_line(&mut Full, "loader", "a line the log can't take", false);
        write_line(&mut Full, "loader", "a line the log can't take", true);
    }

    #[test]
    fn a_line_names_its_kind() {
        let mut out = Vec::new();
        write_line(&mut out, "loader", "ready", false);
        write_line(&mut out, "crash", "gone", true);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "[loader] ready\n\x1b[31;1m[crash]\x1b[0m gone\n"
        );
    }
}
