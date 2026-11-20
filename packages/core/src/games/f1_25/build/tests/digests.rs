use std::time::{Duration, SystemTime};

use super::super::digests::Digests;
use super::super::inputs::source_digests_path;
use super::*;

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-digests-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn sha(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn set_modified(path: &Path, at: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

fn digest(cache: &Path, file: &Path) -> (String, usize) {
    let mut d = Digests::load(cache.to_owned()).settled_after(Duration::ZERO);
    let meta = std::fs::metadata(file).unwrap();
    let sha = d.sha256(file, &meta).unwrap();
    d.save();
    (sha, d.read)
}

/// The 2013 season addon's registry file was restored with `cp` over itself: same inode, same folder
/// times. Here the restore also keeps the size and puts the old modified time back, as `cp -p` would, so
/// only the change time tells the files apart.
#[test]
fn a_source_restored_in_place_with_its_old_size_and_modified_time_is_read_again() {
    let root = scratch("restore");
    let (cache, file) = (root.join("digests.json"), root.join("content.json"));
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000);
    std::fs::write(&file, br#"{"vehicle":["cars/teams/common.erp"]}"#).unwrap();
    set_modified(&file, old);
    let (first, read) = digest(&cache, &file);
    assert_eq!(
        (first.as_str(), read),
        (sha(br#"{"vehicle":["cars/teams/common.erp"]}"#).as_str(), 1)
    );
    assert_eq!(
        digest(&cache, &file),
        (first.clone(), 0),
        "an unchanged file is not read again"
    );

    std::thread::sleep(Duration::from_millis(50));
    let restored = br#"{"vehicle":["cars/teams/lotus2.erp"]}"#;
    std::fs::write(&file, restored).unwrap();
    set_modified(&file, old);
    let meta = std::fs::metadata(&file).unwrap();
    assert_eq!(
        meta.len() as usize,
        br#"{"vehicle":["cars/teams/common.erp"]}"#.len()
    );
    assert_eq!(meta.modified().unwrap(), old);
    assert_eq!(digest(&cache, &file), (sha(restored), 1));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_digest_is_kept_only_once_its_file_has_settled() {
    let root = scratch("settle");
    let (cache, file) = (root.join("digests.json"), root.join("a.json"));
    std::fs::write(&file, b"fresh").unwrap();
    for _ in 0..2 {
        let mut d = Digests::load(cache.clone());
        d.sha256(&file, &std::fs::metadata(&file).unwrap()).unwrap();
        d.save();
        assert_eq!(
            d.read, 1,
            "a file written just now may change again within its clock's tick"
        );
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn kept_digests_of_another_version_or_unreadable_are_dropped_whole() {
    let root = scratch("version");
    let (cache, file) = (root.join("digests.json"), root.join("a.json"));
    std::fs::write(&file, b"a").unwrap();
    assert_eq!(digest(&cache, &file).1, 1);
    assert_eq!(digest(&cache, &file).1, 0);
    let text = std::fs::read_to_string(&cache).unwrap();
    std::fs::write(&cache, text.replace("\"version\": 1", "\"version\": 2")).unwrap();
    assert_eq!(digest(&cache, &file).1, 1);
    std::fs::write(&cache, b"{ not json").unwrap();
    assert_eq!(digest(&cache, &file).1, 1);
    assert_eq!(digest(&cache, &file).1, 0);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn every_spelling_of_a_windows_path_keeps_its_digest_under_one_key() {
    use super::super::digests::key;
    let walked = r"C:\Users\Ann\AppData\Roaming\Swofty\addon-data\test.pack\assets\a.json";
    for named in [
        r"C:\Users\Ann\AppData\Roaming\Swofty\addon-data\test.pack\assets/a.json",
        r"C:\Users\Ann\AppData\Roaming\Swofty\addon-data\test.pack\Assets\A.json",
        "c:/users/ann/appdata/roaming/swofty/addon-data/test.pack/assets/a.json",
    ] {
        assert_eq!(key(named, true), key(walked, true), "{named}");
    }
    assert_ne!(
        key("/home/ann/assets/A.json", false),
        key("/home/ann/assets/a.json", false)
    );
    assert_ne!(
        key(r"/home/ann/assets\a.json", false),
        key("/home/ann/assets/a.json", false)
    );
}

#[test]
fn kept_digests_leave_the_build_hash_as_reading_every_source_makes_it() {
    let root = scratch("hash");
    let store = Store::new(&root);
    let dir = store.addon_data_dir("test.pack").join("assets");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.json"), b"a").unwrap();
    std::fs::write(dir.join("big.bin"), vec![3u8; (1 << 20) + 1]).unwrap();
    let ops: Vec<_> = ["assets", "assets/a.json", "assets/big.bin"]
        .into_iter()
        .map(|path| {
            AssetOp::File(FileOp::Put {
                source: Source {
                    root: Root::Data,
                    path: path.into(),
                },
                target: "out.bin".into(),
            })
        })
        .collect();
    let cache = root.join("digests.json");
    let hashed = |kept: bool| {
        let mut h = match kept {
            true => BuildHash::with_digests(Digests::load(cache.clone()).settled_after(Duration::ZERO)),
            false => BuildHash::new(),
        };
        h.sources(&store, "test.pack", &ops).unwrap();
        h.save_digests();
        let read = h.digests().read;
        (h.finalize(), read)
    };
    let (plain, _) = hashed(false);
    assert_eq!(
        hashed(true),
        (plain, 1),
        "a.json is read once, though two ops name it"
    );
    assert_eq!(hashed(true), (plain, 0));
    std::thread::sleep(Duration::from_millis(50));
    std::fs::write(dir.join("a.json"), b"b").unwrap();
    let (changed, read) = hashed(true);
    assert_eq!((changed, read), (hashed(false).0, 1));
    assert_ne!(changed, plain);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_build_names_the_same_overlay_with_its_kept_digests_or_without_them() {
    if loader_dir().is_some() {
        return;
    }
    let (root, store, game) = super::reuse::reuse_fixture("digests");
    let first = build(&store, "default", &game).unwrap();
    let kept = source_digests_path(&store, "default");
    assert_eq!(
        build(&store, "default", &game).unwrap().buildhash,
        first.buildhash
    );
    std::fs::remove_file(&kept).ok();
    assert_eq!(
        build(&store, "default", &game).unwrap().buildhash,
        first.buildhash
    );
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(&kept, b"{ not json").unwrap();
    assert_eq!(
        build(&store, "default", &game).unwrap().buildhash,
        first.buildhash
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_careers_file_and_the_list_state_are_part_of_the_build_hash() {
    use super::super::inputs::stamp_resume_list;
    let root = scratch("resume-list");
    let game = crate::install::Game {
        dir: root.join("F1 25"),
        user_reg: None,
        identity: crate::game_folder::baseline::Identity {
            build_id: "1".into(),
            exe_sha256: "0".repeat(64),
        },
    };
    let hash = |wanted: bool| {
        let mut h = BuildHash::new();
        stamp_resume_list(&mut h, &root, "default", &game, wanted).unwrap();
        h.finish()
    };
    let absent = hash(true);
    assert_ne!(absent, hash(false));
    let file = crate::stdlib::careers::data_path(&root);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, b"[]").unwrap();
    let empty = hash(true);
    assert_ne!(empty, absent);
    assert_eq!(empty, hash(true));
    std::fs::write(&file, b"[{}]").unwrap();
    assert_ne!(hash(true), empty);
    assert_eq!(hash(false), {
        std::fs::remove_file(&file).unwrap();
        hash(false)
    });
    std::fs::remove_dir_all(&root).ok();
}
