use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use swofty_core::GAME_EXE;
use swofty_core::diagnostics::crash::{LogScan, StartFailure, Symbols, fatal_name};
use swofty_core::session::SessionRecord;
use swofty_i18n::trf;

pub(super) const KB: &str = include_str!("../../../../../re/db/function_names.tsv");

pub fn crash_summary(session: &SessionRecord, game: Option<&Path>) -> Result<String, StartFailure> {
    let mut symbols = Symbols::parse_kb(KB);
    if let Some(dir) = game {
        symbols.load_pdata(&dir.join(GAME_EXE)).ok();
    }
    let mut scan = LogScan::default();
    if let Ok(f) = File::open(session.wine_log()) {
        for line in BufReader::new(f).split(b'\n').map_while(Result::ok) {
            scan.feed(&String::from_utf8_lossy(&line));
        }
    }
    if let Some(failure) = scan.start_failure {
        return Err(failure);
    }
    let code = session.exit_code.unwrap_or_default();
    let fast_fail = swofty_core::diagnostics::fast_fail::session_lines(&session.dir);
    if !fast_fail.is_empty() {
        return Ok(fast_fail.join("\n"));
    }
    Ok(match scan.last_fatal {
        Some((c, addr)) => trf!(
            "exit code {code}, last {exception} at {place}",
            code = code,
            exception = fatal_name(c).unwrap_or("exception"),
            place = scan.describe(addr, &symbols)
        ),
        None => trf!("exit code {code}", code = code),
    })
}
