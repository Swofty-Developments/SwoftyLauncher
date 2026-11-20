use super::Diagnosis;
use super::evidence::Evidence;

/// The start of the line the core logs when the game can't create files in
/// the team logo cache (`apply/content/logo_deliver.hpp`).
pub(super) const LOGOS_UNWRITABLE: &str = "team.logo: the game can't create files in ";

/// The folder the team logo build could not write in and the Windows error,
/// from the core's line.
pub(super) fn logos_unwritable(loader: &str) -> Option<(String, String)> {
    loader.lines().find_map(|l| {
        let rest = &l[l.find(LOGOS_UNWRITABLE)? + LOGOS_UNWRITABLE.len()..];
        let (dir, rest) = rest.split_once(" (error ")?;
        let (code, _) = rest.split_once(')')?;
        Some((dir.trim_end_matches('/').to_owned(), code.to_owned()))
    })
}

/// A session whose team logos were never built: the game could not create
/// files in the folder the core builds them in.
pub fn unbuilt_logos(e: &Evidence) -> Option<Diagnosis> {
    let (dir, code) = logos_unwritable(&e.loader)?;
    let dir = dir.replace('/', "\\");
    Some(Diagnosis {
        id: "logos_not_built",
        title: "Your addons' team logos were not built".to_owned(),
        what: format!(
            "F1 25 could not create files in {dir} in its game folder (error {code}), so the team logos and \
             badges your addons add were missing in this session. Everything else in your addons loaded."
        ),
        steps: vec![
            "Close F1 25, run Swofty Launcher as administrator and press Play: the launcher lets F1 25 \
             write in that folder before it starts."
                .to_owned(),
            "If the logos are still missing, press Send Logs so the launcher's lines about that folder \
             reach support."
                .to_owned(),
        ],
    })
}
