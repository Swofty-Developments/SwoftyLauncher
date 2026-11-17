use std::io::IsTerminal;

pub(crate) fn info(line: &str) {
    say("info", line);
}

pub(crate) fn say(kind: &str, text: &str) {
    let colour = match kind {
        "loader" => "36",
        "core" => "32",
        "wine" => "33",
        "crash" => "31;1",
        _ => "90",
    };
    if std::io::stdout().is_terminal() {
        println!("\x1b[{colour}m[{kind}]\x1b[0m {text}");
    } else {
        println!("[{kind}] {text}");
    }
}
