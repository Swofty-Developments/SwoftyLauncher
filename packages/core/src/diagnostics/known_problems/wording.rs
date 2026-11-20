use super::evidence::Evidence;

/// The files a step names before it says how many more there are.
pub(super) const NAMED_FILES: usize = 6;

pub(super) fn list(names: &[String]) -> String {
    let shown: Vec<&str> = names.iter().take(NAMED_FILES).map(String::as_str).collect();
    let more = names.len().saturating_sub(NAMED_FILES);
    match (shown.as_slice(), more) {
        ([one], 0) => (*one).to_owned(),
        (_, 0) => {
            let (last, rest) = shown.split_last().expect("at least two names");
            format!("{} and {last}", rest.join(", "))
        }
        _ => format!("{} and {more} more", shown.join(", ")),
    }
}

pub(super) const VERIFY: &str = "Verify F1 25's files: in Steam, right-click F1 25, Properties, Installed Files, Verify integrity \
                      of game files. For the EA app copy, use Repair in the EA app.";

pub(super) const SEND_LOGS: &str = "Press Send Logs in the launcher so this reaches the developers.";

pub(super) fn name(e: &Evidence, id: &str) -> String {
    e.names.get(id).cloned().unwrap_or_else(|| id.to_owned())
}
