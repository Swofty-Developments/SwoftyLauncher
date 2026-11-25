//! A copy of the built overlay on the game's own drive. Hard links cannot
//! cross drives, so when the data folder is on another drive Play links the
//! game folder's files to this copy instead of copying every file again. The
//! copy is kept between sessions and brought up to date before each install.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The folder in the game folder that holds the cache. Not inside the loader
/// folder, which a restore deletes.
pub const DIR: &str = "swofty-cache";
const OVERLAY: &str = "overlay";
const ORIGINALS: &str = "originals";
const INDEX: &str = "index.json";
/// Room left on the game's drive after the cache is filled.
pub const MARGIN: u64 = 2 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    len: u64,
    modified: u128,
}

/// What a sync did, and where each overlay file is in the cache.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    pub copied: usize,
    pub reused: usize,
    pub removed: usize,
    pub files: BTreeMap<String, PathBuf>,
}

pub fn dir(game: &Path) -> PathBuf {
    game.join(DIR)
}

pub fn root(game: &Path) -> PathBuf {
    dir(game).join(OVERLAY)
}

/// The folder an install moves the game files it replaces into, each named by
/// its SHA-256, so the rollback renames them back instead of copying them from
/// the backup.
pub fn originals(game: &Path) -> PathBuf {
    dir(game).join(ORIGINALS)
}

fn index_path(game: &Path) -> PathBuf {
    dir(game).join(INDEX)
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(Stamp {
        len: meta.len(),
        modified,
    })
}

/// A cached file: the source it was copied from, that source's stamp, the
/// copy as written, and the SHA-256 the install named for it. A copy the game
/// wrote through its link no longer matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    from: PathBuf,
    source: Stamp,
    copy: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

/// A file the cache should hold: its game path, the file that goes there, and that file's SHA-256 when the
/// caller knows it.
pub type Wanted<'a> = (&'a str, &'a Path, Option<&'a str>);

fn load(game: &Path) -> BTreeMap<String, Kept> {
    std::fs::read(index_path(game))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(game: &Path, index: &BTreeMap<String, Kept>) -> std::io::Result<()> {
    let path = index_path(game);
    std::fs::create_dir_all(dir(game))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(index).unwrap_or_default())?;
    std::fs::rename(&tmp, &path)
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn prune_empty(dir: &Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            prune_empty(&e.path());
            std::fs::remove_dir(e.path()).ok();
        }
    }
}

/// The files in the cache at `cache` whose game path `wanted` does not take.
fn unwanted(cache: &Path, wanted: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut present = Vec::new();
    files(cache, &mut present);
    present.retain(|path| {
        !path
            .strip_prefix(cache)
            .is_ok_and(|r| wanted(r.to_string_lossy().replace('\\', "/").as_str()))
    });
    present
}

/// Whether the cached copy `kept`, whose file is stamped `copy`, can stay for `src` stamped `now`: the copy
/// is untouched since it was written, and was copied from `src` as it is now or holds the SHA-256 `sha`
/// names for it.
fn keeps(
    kept: &Kept,
    copy: Option<Stamp>,
    src: &Path,
    now: Stamp,
    sha: impl FnOnce() -> Option<String>,
) -> bool {
    copy == Some(kept.copy)
        && ((kept.from == src && kept.source == now)
            || kept.sha256.as_ref().is_some_and(|k| sha().as_ref() == Some(k)))
}

/// What a [`sync`] does on the game's drive: the bytes it copies into the cache, and the bytes of the cached
/// files it deletes before it checks the free space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    pub copies: u64,
    pub frees: u64,
}

impl Pending {
    /// The free space the game's drive needs for the files Play hands the sync: what the sync copies plus
    /// [`MARGIN`], or `copying` when copying those files straight into the game folder, as Play does when the
    /// sync fails, needs less; less what the sync deletes before either.
    pub fn need(self, copying: u64) -> u64 {
        self.copies
            .saturating_add(MARGIN)
            .min(copying)
            .saturating_sub(self.frees)
    }
}

/// What a [`sync`] of `sources` would do, by the same stamps and SHA-256s, without changing anything.
/// `sha` names a source's SHA-256; it is asked only when a cached copy from another source may hold the
/// same bytes.
pub fn pending(
    game: &Path,
    sources: &[(&str, &Path)],
    sha: &mut dyn FnMut(&Path) -> Option<String>,
) -> Pending {
    let cache = root(game);
    let wanted: BTreeMap<&str, &Path> = sources.iter().copied().collect();
    let frees = unwanted(&cache, |rel| wanted.contains_key(rel))
        .iter()
        .map(|p| std::fs::metadata(p).map_or(0, |m| m.len()))
        .sum();
    let index = load(game);
    let copies = wanted
        .iter()
        .filter_map(|(k, src)| {
            let now = stamp(src)?;
            let kept = index
                .get(*k)
                .is_some_and(|i| keeps(i, stamp(&cache.join(k)), src, now, || sha(src)));
            (!kept).then_some(now.len)
        })
        .sum();
    Pending { copies, frees }
}

/// Brings the cache in `game` up to date with `sources`: copies the files that
/// are new, come from another source, or whose size or modified time changed,
/// unless the cached copy is untouched since it was written and holds the
/// SHA-256 the source is named with, as a rebuild that wrote the same bytes
/// again does. Leaves the rest, and deletes what the overlay no longer has. Fails
/// with the reason when the game's drive hasn't room for what is left to copy
/// plus `MARGIN`, or a copy fails; the cache then holds what was copied.
pub fn sync(
    game: &Path,
    sources: &[Wanted<'_>],
    free_space: fn(&Path) -> Option<u64>,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Synced, String> {
    let cache = root(game);
    let wanted: BTreeMap<&str, (&Path, Option<&str>)> = sources
        .iter()
        .map(|(rel, src, sha)| (*rel, (*src, *sha)))
        .collect();
    let mut out = Synced::default();
    for path in unwanted(&cache, |rel| wanted.contains_key(rel)) {
        if std::fs::remove_file(&path).is_ok() {
            out.removed += 1;
        }
    }
    prune_empty(&cache);
    let mut index = load(game);
    index.retain(|k, _| wanted.contains_key(k.as_str()));
    let mut todo = Vec::new();
    for (k, (src, sha)) in &wanted {
        let now = stamp(src).ok_or_else(|| format!("cannot read {}", src.display()))?;
        let at = cache.join(k);
        let copy = stamp(&at);
        match index
            .get_mut(*k)
            .filter(|i| keeps(i, copy, src, now, || sha.map(str::to_owned)))
        {
            Some(i) => {
                i.from = src.to_path_buf();
                i.source = now;
                out.reused += 1;
                out.files.insert((*k).to_owned(), at);
            }
            None => {
                index.remove(*k);
                todo.push((*k, *src, *sha, now));
            }
        }
    }
    let need: u64 = todo.iter().map(|(_, _, _, s)| s.len).sum();
    if let Some(free) = free_space(game)
        && free < need.saturating_add(MARGIN)
    {
        save(game, &index).ok();
        return Err(format!(
            "the game drive has {} MB free and the cache needs {} MB more, plus {} MB to spare",
            free >> 20,
            need >> 20,
            MARGIN >> 20
        ));
    }
    let total = todo.len();
    for (n, (k, src, sha, now)) in todo.into_iter().enumerate() {
        let at = cache.join(k);
        let sha256 = sha.filter(|_| stamp(src) == Some(now)).map(str::to_owned);
        let made = at
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| copy_checked(src, &at, sha256.as_deref()));
        let copy = stamp(&at);
        let Some(copy) = copy.filter(|_| made.is_ok()) else {
            save(game, &index).ok();
            let why = made.err().map_or("it is missing".to_owned(), |e| e.to_string());
            return Err(format!("cannot copy {}: {why}", src.display()));
        };
        index.insert(
            k.to_owned(),
            Kept {
                from: src.to_path_buf(),
                source: now,
                copy,
                sha256,
            },
        );
        out.files.insert(k.to_owned(), at);
        out.copied += 1;
        progress(n + 1, total);
    }
    save(game, &index).map_err(|e| format!("cannot write the cache index: {e}"))?;
    Ok(out)
}

/// Copies `src` to `at`, and copies it once more when the copy is not the source's size or, for an archive whose
/// SHA-256 `sha` names, not its bytes; a copy that is still not the source fails. The game takes an archive
/// that differs from the one built for it into a decompression loop it never leaves.
fn copy_checked(src: &Path, at: &Path, sha: Option<&str>) -> std::io::Result<()> {
    let intact = || {
        let size =
            std::fs::metadata(src).ok().map(|m| m.len()) == std::fs::metadata(at).ok().map(|m| m.len());
        let bytes = match sha {
            Some(want) if at.extension().is_some_and(|x| x == "erp") => {
                crate::game_folder::baseline::sha256_file(at).is_ok_and(|got| got == want)
            }
            _ => true,
        };
        size && bytes
    };
    for _ in 0..2 {
        crate::install::copy_replacing(src, at)?;
        if intact() {
            return Ok(());
        }
    }
    std::fs::remove_file(at).ok();
    Err(std::io::Error::other(
        "the copy on the game drive differs from the file it was copied from, twice",
    ))
}

pub fn size(game: &Path) -> u64 {
    crate::data::space::dir_size(&dir(game))
}

/// Deletes the cache; the bytes it held.
pub fn remove(game: &Path) -> u64 {
    let held = size(game);
    let path = dir(game);
    if path.exists() && std::fs::remove_dir_all(&path).is_err() {
        let mut left = Vec::new();
        files(&path, &mut left);
        for f in left {
            crate::clear_readonly(&f);
            std::fs::remove_file(&f).ok();
        }
        prune_empty(&path);
        std::fs::remove_dir(&path).ok();
    }
    held.saturating_sub(size(game))
}

/// Whether `a` and `b` are on different volumes, when both are known.
pub fn apart(
    a: Option<crate::platform::disk::Volume>,
    b: Option<crate::platform::disk::Volume>,
) -> Option<bool> {
    Some(a?.id != b?.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("swofty-linkcache-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn roomy(_: &Path) -> Option<u64> {
        Some(1 << 40)
    }

    fn run(built: &Path, game: &Path, rels: &[&str]) -> Result<Synced, String> {
        let srcs: Vec<PathBuf> = rels.iter().map(|r| built.join(r)).collect();
        let refs: Vec<Wanted> = rels
            .iter()
            .copied()
            .zip(srcs.iter().map(|p| p.as_path()))
            .map(|(rel, src)| (rel, src, None))
            .collect();
        sync(game, &refs, roomy, &mut |_, _| {})
    }

    #[test]
    fn a_copied_archive_that_is_not_the_bytes_its_digest_names_is_refused_and_removed() {
        use sha2::{Digest, Sha256};
        let root = tmp("checked");
        put(&root.join("a/screens.erp"), "archive bytes");
        let want: String = Sha256::digest(b"archive bytes")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let at = root.join("b/screens.erp");
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        copy_checked(&root.join("a/screens.erp"), &at, Some(&want)).unwrap();
        assert_eq!(std::fs::read(&at).unwrap(), b"archive bytes");
        let err = copy_checked(&root.join("a/screens.erp"), &at, Some("00")).unwrap_err();
        assert!(err.to_string().contains("differs"), "{err}");
        assert!(!at.exists());
        put(&root.join("a/other.bin"), "x");
        copy_checked(&root.join("a/other.bin"), &root.join("b/other.bin"), Some("00")).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_files_are_copied_and_unchanged_ones_reused() {
        let root = tmp("new");
        let (built, game) = (root.join("data/cache/overlay/p"), root.join("game"));
        put(&built.join("a.bin"), "one");
        put(&built.join("sub/b.bin"), "two");
        let first = run(&built, &game, &["a.bin", "sub/b.bin"]).unwrap();
        assert_eq!((first.copied, first.reused, first.removed), (2, 0, 0));
        let again = run(&built, &game, &["a.bin", "sub/b.bin"]).unwrap();
        assert_eq!((again.copied, again.reused), (0, 2));
        let at = &again.files["sub/b.bin"];
        assert_eq!(at, &root_of(&game).join("sub/b.bin"));
        assert_eq!(std::fs::read_to_string(at).unwrap(), "two");
        std::fs::remove_dir_all(&root).ok();
    }

    fn root_of(game: &Path) -> PathBuf {
        super::root(game)
    }

    #[test]
    fn a_changed_file_is_copied_again() {
        let root = tmp("changed");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        put(&built.join("b.bin"), "keep");
        run(&built, &game, &["a.bin", "b.bin"]).unwrap();
        put(&built.join("a.bin"), "three");
        let s = run(&built, &game, &["a.bin", "b.bin"]).unwrap();
        assert_eq!((s.copied, s.reused), (1, 1));
        assert_eq!(
            std::fs::read_to_string(root_of(&game).join("a.bin")).unwrap(),
            "three"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_from_another_source_is_copied_again() {
        let root = tmp("source");
        let game = root.join("game");
        let (a, b) = (root.join("a/db.bin"), root.join("b/db.bin"));
        put(&a, "db-A");
        put(&b, "db-B");
        let when = std::fs::metadata(&a).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&b)
            .unwrap()
            .set_modified(when)
            .unwrap();
        sync(&game, &[("db.bin", a.as_path(), None)], roomy, &mut |_, _| {}).unwrap();
        let s = sync(&game, &[("db.bin", b.as_path(), None)], roomy, &mut |_, _| {}).unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        assert_eq!(
            std::fs::read_to_string(root_of(&game).join("db.bin")).unwrap(),
            "db-B"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_file_the_overlay_no_longer_has_is_deleted() {
        let root = tmp("stale");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        put(&built.join("dir/b.bin"), "two");
        run(&built, &game, &["a.bin", "dir/b.bin"]).unwrap();
        let s = run(&built, &game, &["a.bin"]).unwrap();
        assert_eq!((s.copied, s.reused, s.removed), (0, 1, 1));
        assert!(!root_of(&game).join("dir/b.bin").exists());
        assert!(!root_of(&game).join("dir").exists());
        assert!(root_of(&game).join("a.bin").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_cache_file_the_game_wrote_through_its_link_is_copied_again() {
        let root = tmp("written");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        run(&built, &game, &["a.bin"]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        put(&root_of(&game).join("a.bin"), "two");
        let s = run(&built, &game, &["a.bin"]).unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        assert_eq!(
            std::fs::read_to_string(root_of(&game).join("a.bin")).unwrap(),
            "one"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_damaged_cache_file_is_copied_again() {
        let root = tmp("damaged");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        run(&built, &game, &["a.bin"]).unwrap();
        std::fs::remove_file(root_of(&game).join("a.bin")).unwrap();
        let s = run(&built, &game, &["a.bin"]).unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        std::fs::remove_dir_all(&root).ok();
    }

    fn sha(body: &str) -> String {
        crate::game_folder::baseline::sha256_bytes(body.as_bytes())
    }

    #[test]
    fn a_rebuild_that_wrote_the_same_bytes_elsewhere_copies_nothing() {
        let root = tmp("same-bytes");
        let game = root.join("game");
        let (a, b) = (root.join("overlay/aaaa"), root.join("overlay/bbbb"));
        for dir in [&a, &b] {
            put(&dir.join("db.bin"), "database");
            put(&dir.join("ui/menu.erp"), "menus");
        }
        put(&b.join("new.bin"), "new");
        let wanted = |dir: &Path, rels: &[(&'static str, &'static str)]| -> Vec<(String, PathBuf, String)> {
            rels.iter()
                .map(|(rel, body)| ((*rel).to_owned(), dir.join(rel), sha(body)))
                .collect()
        };
        let call = |w: Vec<(String, PathBuf, String)>| {
            let v: Vec<Wanted> = w
                .iter()
                .map(|(rel, src, sha)| (rel.as_str(), src.as_path(), Some(sha.as_str())))
                .collect();
            sync(&game, &v, roomy, &mut |_, _| {}).unwrap()
        };
        let first = call(wanted(&a, &[("db.bin", "database"), ("ui/menu.erp", "menus")]));
        assert_eq!((first.copied, first.reused), (2, 0));
        let second = call(wanted(
            &b,
            &[
                ("db.bin", "database"),
                ("ui/menu.erp", "menus"),
                ("new.bin", "new"),
            ],
        ));
        assert_eq!((second.copied, second.reused), (1, 2));
        let third = call(wanted(
            &b,
            &[
                ("db.bin", "database"),
                ("ui/menu.erp", "menus"),
                ("new.bin", "new"),
            ],
        ));
        assert_eq!((third.copied, third.reused), (0, 3));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_named_digest_never_keeps_a_copy_that_differs_or_was_written_through_its_link() {
        let root = tmp("digest-guard");
        let game = root.join("game");
        let (a, b) = (root.join("a/db.bin"), root.join("b/db.bin"));
        put(&a, "db-A");
        put(&b, "db-B");
        let (sha_a, sha_b) = (sha("db-A"), sha("db-B"));
        sync(
            &game,
            &[("db.bin", a.as_path(), Some(&sha_a))],
            roomy,
            &mut |_, _| {},
        )
        .unwrap();
        let s = sync(
            &game,
            &[("db.bin", b.as_path(), Some(&sha_b))],
            roomy,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        assert_eq!(
            std::fs::read_to_string(root_of(&game).join("db.bin")).unwrap(),
            "db-B"
        );

        std::thread::sleep(std::time::Duration::from_millis(30));
        put(&root_of(&game).join("db.bin"), "db-X");
        let s = sync(
            &game,
            &[("db.bin", a.as_path(), Some(&sha_a))],
            roomy,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        std::thread::sleep(std::time::Duration::from_millis(30));
        put(&root_of(&game).join("db.bin"), "db-Y");
        let s = sync(
            &game,
            &[("db.bin", b.as_path(), Some(&sha_a))],
            roomy,
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!((s.copied, s.reused), (1, 0));
        assert_eq!(
            std::fs::read_to_string(root_of(&game).join("db.bin")).unwrap(),
            "db-B"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn pending_counts_what_a_sync_copies_and_deletes() {
        let root = tmp("pending");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        put(&built.join("sub/b.bin"), "two!");
        let (a, b) = (built.join("a.bin"), built.join("sub/b.bin"));
        let both = [("a.bin", a.as_path()), ("sub/b.bin", b.as_path())];
        let mut unnamed = |_: &Path| None;
        let p = |copies, frees| Pending { copies, frees };
        assert_eq!(pending(&game, &both, &mut unnamed), p(7, 0), "an empty cache");
        run(&built, &game, &["a.bin", "sub/b.bin"]).unwrap();
        assert_eq!(pending(&game, &both, &mut unnamed), p(0, 0), "all cached");
        put(&built.join("a.bin"), "three");
        assert_eq!(pending(&game, &both, &mut unnamed), p(5, 0), "one changed");
        assert_eq!(run(&built, &game, &["a.bin", "sub/b.bin"]).unwrap().copied, 1);
        assert_eq!(
            pending(&game, &both[..1], &mut unnamed),
            p(0, 4),
            "one no longer placed"
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
        put(&root_of(&game).join("a.bin"), "three");
        assert_eq!(
            pending(&game, &both, &mut unnamed),
            p(5, 0),
            "a copy written through its link"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn pending_keeps_the_same_bytes_from_a_rebuild_by_their_digest_and_asks_for_it_only_then() {
        let root = tmp("pending-digest");
        let game = root.join("game");
        let (a, b) = (root.join("a/db.bin"), root.join("b/db.bin"));
        put(&a, "database");
        put(&b, "database");
        let digest = sha("database");
        sync(
            &game,
            &[("db.bin", a.as_path(), Some(&digest))],
            roomy,
            &mut |_, _| {},
        )
        .unwrap();
        let mut asked = 0;
        let mut named = |_: &Path| {
            asked += 1;
            Some(digest.clone())
        };
        assert_eq!(pending(&game, &[("db.bin", a.as_path())], &mut named).copies, 0);
        assert_eq!(pending(&game, &[("db.bin", b.as_path())], &mut named).copies, 0);
        assert_eq!(asked, 1);
        let other = sha("another");
        let mut differs = |_: &Path| Some(other.clone());
        assert_eq!(pending(&game, &[("db.bin", b.as_path())], &mut differs).copies, 8);
        assert_eq!(
            pending(&game, &[("db.bin", b.as_path())], &mut |_| None).copies,
            8
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_sync_needs_what_it_copies_plus_the_margin_unless_copying_instead_needs_less() {
        let gb = 1u64 << 30;
        let p = |copies, frees| Pending { copies, frees };
        assert_eq!(p(0, 0).need(14 * gb), MARGIN, "everything cached");
        assert_eq!(p(gb, 0).need(14 * gb), gb + MARGIN, "one file changed");
        assert_eq!(p(14 * gb, 0).need(13 * gb), 13 * gb, "an empty cache");
        assert_eq!(p(gb, 2 * gb).need(14 * gb), gb, "a cached file no longer placed");
        assert_eq!(p(gb, 4 * gb).need(14 * gb), 0);
        assert_eq!(p(0, 0).need(0), 0, "nothing to cache");
    }

    #[test]
    fn too_little_room_on_the_game_drive_fails_and_leaves_the_cache_alone() {
        let root = tmp("space");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "one");
        let src = built.join("a.bin");
        let refs = [("a.bin", src.as_path(), None)];
        let err = sync(&game, &refs, |_| Some(MARGIN), &mut |_, _| {}).unwrap_err();
        assert!(err.contains("game drive"), "{err}");
        assert!(!root_of(&game).join("a.bin").exists());
        let ok = sync(&game, &refs, |_| Some(MARGIN + 3), &mut |_, _| {}).unwrap();
        assert_eq!(ok.copied, 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn removing_the_cache_reports_what_it_held() {
        let root = tmp("remove");
        let (built, game) = (root.join("built"), root.join("game"));
        put(&built.join("a.bin"), "12345");
        run(&built, &game, &["a.bin"]).unwrap();
        assert!(size(&game) >= 5);
        assert!(remove(&game) >= 5);
        assert!(!dir(&game).exists());
        assert_eq!(remove(&game), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn volumes_are_compared_by_id() {
        let v = |id: &str| {
            Some(crate::platform::disk::Volume {
                id: id.into(),
                name: id.into(),
            })
        };
        assert_eq!(apart(v("1"), v("2")), Some(true));
        assert_eq!(apart(v("1"), v("1")), Some(false));
        assert_eq!(apart(v("1"), None), None);
    }
}
