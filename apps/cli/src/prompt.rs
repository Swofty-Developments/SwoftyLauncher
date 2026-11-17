use std::io::{IsTerminal, Write};

use anyhow::Result;

pub(crate) fn interactive() -> bool {
    std::io::stdin().is_terminal()
}

pub(crate) fn ask(prompt: &str) -> Result<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

pub(crate) fn yes(question: &str) -> Result<bool> {
    Ok(matches!(
        ask(&format!("{question} [y/N] "))?.as_str(),
        "y" | "Y" | "yes"
    ))
}
