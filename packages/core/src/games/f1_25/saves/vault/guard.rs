//! The guard against losing a career to a game that does not know its mode. F1 25 started without the
//! addons (Steam's Play button, with the game folder rolled back) drops the career containers no loaded game
//! mode claims when it saves, and Steam Cloud uploads that save. The launcher reads the plain store the core
//! writes from inside the game: a container or career identity (container name, then slot guid) that the
//! last good save generation had and the newest generation lacks is lost. Careers the core put back at load
//! are not. No byte total decides anything.

use super::{Notice, NoticeKind, Reason, Vault};
use crate::saves::plain::{self, Plain};

const DISMISSED: &str = "loss-dismissed";

/// Containers and careers the newest plain generation lacks, and the vault snapshot a launcher session left
/// last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loss {
    pub source: String,
    pub snapshot: Option<String>,
    pub at: i64,
    pub plain: plain::Loss,
}

impl Loss {
    pub fn generation(&self) -> u64 {
        self.plain.good
    }

    pub fn names(&self) -> Vec<String> {
        self.plain.names()
    }

    pub fn containers(&self) -> Vec<String> {
        self.plain.containers()
    }

    pub fn notice(&self, now: i64) -> Notice {
        Notice {
            kind: NoticeKind::Lost,
            source: self.source.clone(),
            snapshot: self.snapshot.clone().unwrap_or_default(),
            kept_at: self.at,
            generation: self.plain.good,
            containers: self.containers(),
            modes: self.plain.modes(),
            names: self.names(),
            at: now,
        }
    }
}

fn left_by_launcher(r: Reason) -> bool {
    matches!(r, Reason::AfterSession | Reason::BeforeSession | Reason::Restored)
}

/// The `[saves] put back` lines of the newest session's loader log.
fn put_back_log(data: &std::path::Path) -> Vec<plain::PutBack> {
    let sessions = crate::store::Store::new(data.to_path_buf()).sessions_dir();
    let Ok(read) = std::fs::read_dir(&sessions) else {
        return Vec::new();
    };
    let mut dirs: Vec<std::path::PathBuf> = read.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    let Some(newest) = dirs.last() else {
        return Vec::new();
    };
    ["loader.log", "game-logs/loader.log"]
        .iter()
        .filter_map(|rel| std::fs::read_to_string(newest.join(rel)).ok())
        .flat_map(|text| plain::put_backs(&text))
        .collect()
}

impl Vault {
    pub fn probable_loss(&self) -> Option<Loss> {
        let found = Plain::of_source(&self.data, &self.source).loss(&put_back_log(&self.data))?;
        if std::fs::read_to_string(self.dir.join(DISMISSED)).is_ok_and(|d| d.trim() == found.key()) {
            return None;
        }
        let snapshot = self
            .list()
            .ok()?
            .into_iter()
            .find(|s| left_by_launcher(s.reason))
            .map(|s| s.id);
        Some(Loss {
            source: self.source.clone(),
            snapshot,
            at: found.at,
            plain: found,
        })
    }

    pub fn dismiss_loss(&self) -> crate::Result<()> {
        let _lock = self.lock()?;
        let Some(loss) = self.probable_loss() else {
            return Ok(());
        };
        let path = self.dir.join(DISMISSED);
        std::fs::write(&path, loss.plain.key()).map_err(crate::io_err(&path))
    }
}

/// Whether a profile's Saves page warns that its addon careers live in the Steam save.
pub fn steam_save_warning(
    vanilla: bool,
    launcher_saves: bool,
    addon_careers: &[super::careers::Brief],
) -> bool {
    !vanilla && !launcher_saves && !addon_careers.is_empty()
}

pub const STEAM_SAVE_WARNING: &str = "This profile's addon careers live in your Steam save. Starting F1 25 from Steam without the launcher can remove them. A Play from the launcher puts back the careers its plain copy holds; launcher saves keep them out of Steam's reach.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::plain::fixture::{Gen, career_gen, current, pair, write};
    use crate::saves::vault::{Layout, Location, Meta, Snapshot, Taken};
    use crate::stdlib::careers::parse;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("vault-guard-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const WITH: &str = r#"{"careers":[
        {"career":"driver_career:a1","slot":"a1","status":"present","mode":"driver_career"},
        {"career":"progressive:7f","slot":"7f","status":"present","mode":"gm_driver_career_progressive","label":"Progressive 2016","season":2016,"round":7,"addons":[{"id":"swofty.progressive"}]}
    ]}"#;

    struct World {
        data: std::path::PathBuf,
        loc: Location,
        vault: Vault,
        plain: Plain,
    }

    fn world(name: &str) -> World {
        let data = dir(name);
        let remote = data
            .join("steam")
            .join("userdata")
            .join("843502855")
            .join("3059520")
            .join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(remote.join("savegame@profile#NXDSMWW.BWW"), vec![1u8; 4000]).unwrap();
        std::fs::write(remote.join("savegame@profile#QKRHMYXE"), vec![7u8; 416]).unwrap();
        let loc = Location::steam(&remote).unwrap();
        assert_eq!(loc.layout, Layout::Steam);
        let vault = Vault::of(&data, &loc).unwrap();
        let plain = Plain::of_source(&data, &loc.source);
        World {
            data,
            loc,
            vault,
            plain,
        }
    }

    fn take(w: &World, reason: Reason, at: i64) -> Snapshot {
        match w.vault.snapshot(&w.loc, Meta::new(reason), at).unwrap() {
            Taken::New(s) | Taken::Unchanged(s) => s,
            Taken::NoSave => panic!("no save"),
        }
    }

    fn rewrite(w: &World, byte: u8) {
        std::fs::write(w.loc.dir.join("savegame@profile#NXDSMWW.BWW"), vec![byte; 4000]).unwrap();
    }

    fn lost_after_a_stock_play(w: &World) {
        write(&w.plain, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
        write(
            &w.plain,
            &Gen {
                n: 2,
                event: "load",
                session: None,
                containers: pair("driver_career", "game", vec![("0x01", "Stock")]),
            },
        );
        current(&w.plain, 2);
    }

    #[test]
    fn a_container_the_newest_generation_lacks_is_lost_whatever_the_file_sizes_do() {
        let w = world("lost");
        take(&w, Reason::AfterSession, 1_000);
        write(&w.plain, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
        current(&w.plain, 1);
        assert!(w.vault.probable_loss().is_none());
        lost_after_a_stock_play(&w);
        rewrite(&w, 5);
        take(&w, Reason::ForeignExit, 2_000);
        let loss = w.vault.probable_loss().unwrap();
        assert_eq!(loss.generation(), 1);
        assert_eq!(loss.plain.modes(), ["driver_career_progressive"]);
        assert_eq!(loss.containers().len(), 2);
        assert_eq!(loss.names(), ["Jacob, season 2, round 5"]);
        assert!(loss.snapshot.is_some());
        std::fs::remove_dir_all(&w.data).ok();
    }

    #[test]
    fn a_bigger_or_smaller_save_alone_loses_nothing() {
        let w = world("sizes");
        write(&w.plain, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
        write(&w.plain, &career_gen(2, "load", None, &[("0x0a", "Jacob")]));
        current(&w.plain, 2);
        take(&w, Reason::AfterSession, 1_000);
        std::fs::write(w.loc.dir.join("savegame@profile#NXDSMWW.BWW"), vec![3u8; 100]).unwrap();
        w.vault
            .check(&w.loc, Meta::new(Reason::LauncherStart), 2_000)
            .unwrap();
        assert!(w.vault.probable_loss().is_none());
        std::fs::write(w.loc.dir.join("savegame@profile#NXDSMWW.BWW"), vec![3u8; 90_000]).unwrap();
        let c = w
            .vault
            .check(&w.loc, Meta::new(Reason::LauncherStart), 3_000)
            .unwrap();
        assert!(c.notice.is_none() && w.vault.probable_loss().is_none());
        std::fs::remove_dir_all(&w.data).ok();
    }

    #[test]
    fn a_check_raises_the_notice_and_a_put_back_clears_it() {
        let w = world("notice");
        take(&w, Reason::AfterSession, 1_000);
        lost_after_a_stock_play(&w);
        rewrite(&w, 5);
        let c = w
            .vault
            .check(&w.loc, Meta::new(Reason::LauncherStart), 2_000)
            .unwrap();
        let n = c.notice.unwrap();
        assert_eq!(n.kind, NoticeKind::Lost);
        assert_eq!(n.modes, ["driver_career_progressive"]);
        assert_eq!(n.containers.len(), 2);
        assert!(n.text().contains("generation 1"), "{}", n.text());
        assert!(
            n.text().contains("restore-career driver_career_progressive"),
            "{}",
            n.text()
        );
        assert_eq!(Notice::read(&w.data).unwrap(), n);

        let mut put = career_gen(3, "load", None, &[("0x0a", "Jacob")]);
        put.containers[2].1 = "injected";
        put.containers[3].1 = "injected";
        write(&w.plain, &put);
        current(&w.plain, 3);
        let c = w
            .vault
            .check(&w.loc, Meta::new(Reason::LauncherStart), 3_000)
            .unwrap();
        assert!(c.notice.is_none());
        assert!(Notice::read(&w.data).is_none());
        std::fs::remove_dir_all(&w.data).ok();
    }

    #[test]
    fn a_dismissal_holds_until_the_loss_changes() {
        let w = world("dismiss");
        take(&w, Reason::AfterSession, 1_000);
        lost_after_a_stock_play(&w);
        assert!(w.vault.probable_loss().is_some());
        w.vault.dismiss_loss().unwrap();
        assert!(w.vault.probable_loss().is_none());
        write(
            &w.plain,
            &career_gen(3, "save", Some("s2"), &[("0x0a", "Jacob"), ("0x0b", "Ada")]),
        );
        write(
            &w.plain,
            &Gen {
                n: 4,
                event: "load",
                session: None,
                containers: vec![],
            },
        );
        current(&w.plain, 4);
        assert!(
            w.vault.probable_loss().is_some(),
            "a new loss is not the dismissed one"
        );
        std::fs::remove_dir_all(&w.data).ok();
    }

    #[test]
    fn a_session_save_without_a_career_is_a_deletion_not_a_loss() {
        let w = world("deleted");
        write(&w.plain, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
        write(&w.plain, &career_gen(2, "save", Some("s2"), &[]));
        write(&w.plain, &career_gen(3, "load", None, &[]));
        current(&w.plain, 3);
        assert!(w.vault.probable_loss().is_none());
        std::fs::remove_dir_all(&w.data).ok();
    }

    #[test]
    fn the_warning_needs_launcher_saves_off_and_an_addon_career() {
        let held = super::super::careers::addon_careers(&parse(WITH), None);
        assert_eq!(held.len(), 1);
        assert!(steam_save_warning(false, false, &held));
        assert!(!steam_save_warning(false, true, &held));
        assert!(!steam_save_warning(true, false, &held));
        assert!(!steam_save_warning(false, false, &[]));
        assert!(super::super::careers::addon_careers(&parse(WITH), Some("other")).len() == 1);
    }
}
