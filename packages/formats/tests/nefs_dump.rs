use std::io::Cursor;
use std::path::PathBuf;

use aes::Aes256;
use aes::cipher::{BlockCipherEncrypt, KeyInit};
use swofty_formats::nefs::{Archive, MAGIC, VERSION_160};

#[allow(dead_code)]
#[path = "../examples/nefs_dump.rs"]
mod nefs_dump;

const KEY: &[u8; 64] = b"0F1E2D3C4B5A69788796A5B4C3D2E1F00123456789ABCDEFFEDCBA9876543210";
const INTRO: u32 = 128;
const BLOCK: u32 = 1 << 15;

struct Node {
    parent: u32,
    name: &'static str,
    content: Option<Vec<u8>>,
}

fn node(parent: u32, name: &'static str, content: Option<Vec<u8>>) -> Node {
    Node {
        parent,
        name,
        content,
    }
}

fn key() -> [u8; 32] {
    let mut k = [0; 32];
    for (i, pair) in KEY.chunks_exact(2).enumerate() {
        k[i] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    k
}

fn put(out: &mut Vec<u8>, vals: &[u32]) {
    for v in vals {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn archive(nodes: &[Node]) -> (Vec<u8>, Vec<u8>) {
    let cipher = Aes256::new(&key().into());
    let mut data = vec![0xee; 16];
    let mut names = b"game.dat\0".to_vec();
    let mut blocks: Vec<u32> = Vec::new();
    let mut rows = Vec::new();
    for n in nodes {
        let name_at = names.len() as u32;
        names.extend_from_slice(n.name.as_bytes());
        names.push(0);
        match &n.content {
            Some(content) => {
                let start = data.len() as u32;
                let first = blocks.len() as u32;
                for chunk in content.chunks(BLOCK as usize) {
                    let mut c = chunk.to_vec();
                    c.resize(chunk.len().div_ceil(16) * 16, 0);
                    for b in c.chunks_exact_mut(16) {
                        cipher.encrypt_block(b.try_into().unwrap());
                    }
                    data.extend_from_slice(&c);
                    blocks.push(data.len() as u32 - start);
                }
                rows.push((start, first, n.parent, name_at, content.len() as u32));
            }
            None => rows.push((0, 0, n.parent, name_at, 0)),
        }
    }
    let count = nodes.len() as u32;
    let entry_at = INTRO + 0x80;
    let shared_at = entry_at + 20 * count;
    let name_at = shared_at + 20 * count;
    let block_at = name_at + names.len() as u32;
    let volume_at = block_at + 8 * blocks.len() as u32;
    let size = volume_at + 16;

    let mut h = Vec::new();
    h.extend_from_slice(MAGIC);
    h.resize(36, 0);
    h.extend_from_slice(KEY);
    put(&mut h, &[size, VERSION_160, 2, 0]);
    h.resize(INTRO as usize, 0);
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes());
    h.extend_from_slice(&((BLOCK >> 15) as u16).to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes());
    put(
        &mut h,
        &[entry_at, 0, shared_at, 0, name_at, block_at, volume_at, 0],
    );
    h.resize(entry_at as usize, 0);
    for (i, &(start, first, _, _, _)) in rows.iter().enumerate() {
        put(&mut h, &[start, 0, i as u32, first, 0]);
    }
    for &(_, _, parent, name, size) in &rows {
        put(&mut h, &[parent, 0, name, size, 0]);
    }
    h.extend_from_slice(&names);
    for end in blocks {
        h.extend_from_slice(&end.to_le_bytes());
        h.extend_from_slice(&4u16.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
    }
    h.extend_from_slice(&(data.len() as u64).to_le_bytes());
    put(&mut h, &[0, 0]);
    (h, data)
}

fn exe_with(header: &[u8]) -> Vec<u8> {
    let mut exe = vec![0x90; 4096];
    exe.extend_from_slice(header);
    exe.extend_from_slice(&[0xcc; 512]);
    exe
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nefs_dump-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len as u32)
        .map(|i| (i.wrapping_mul(seed) % 251) as u8)
        .collect()
}

fn sample() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let config = pattern(70_000, 7);
    let flow = pattern(1_000, 13);
    let (header, data) = archive(&[
        node(0, "system", None),
        node(0, "config.xml", Some(config.clone())),
        node(2, "game", None),
        node(2, "flow_race.xml", Some(flow.clone())),
    ]);
    (exe_with(&header), data, config, flow)
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn lists_and_extracts_the_kept_files() {
    let (exe, data, config, flow) = sample();
    let dir = scratch("extract");
    std::fs::write(dir.join("F1_25.exe"), &exe).unwrap();
    std::fs::write(dir.join("game.dat"), &data).unwrap();
    let out = dir.join("out");
    let (e, d, o) = (dir.join("F1_25.exe"), dir.join("game.dat"), out.clone());
    let a = args(&[
        e.to_str().unwrap(),
        d.to_str().unwrap(),
        "--out",
        o.to_str().unwrap(),
        "CONFIG",
    ]);
    let mut text = Vec::new();
    nefs_dump::run(&a, &mut text).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(
        text.starts_with("# volume game.dat block 32768 entries 4 kept 1\n"),
        "{text}"
    );
    assert!(text.contains("system/config.xml\t70000\n"), "{text}");
    assert!(!text.contains("flow_race"), "{text}");
    assert_eq!(std::fs::read(out.join("system/config.xml")).unwrap(), config);
    assert!(!out.join("game").exists());

    let mut all = Vec::new();
    nefs_dump::run(
        &args(&[
            e.to_str().unwrap(),
            d.to_str().unwrap(),
            "--out",
            o.to_str().unwrap(),
        ]),
        &mut all,
    )
    .unwrap();
    assert_eq!(std::fs::read(out.join("game/flow_race.xml")).unwrap(), flow);
    assert!(out.join("system").is_dir() && out.join("game").is_dir());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn folders_are_not_listed_as_files() {
    let (exe, _, _, _) = sample();
    let archive = Archive::open_exe(&mut Cursor::new(exe)).unwrap();
    let kept: Vec<&str> = nefs_dump::selected(&archive, &[])
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(kept, ["system/config.xml", "game/flow_race.xml"]);
    let kept: Vec<&str> = nefs_dump::selected(&archive, &args(&["FLOW", "nothing"]))
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(kept, ["game/flow_race.xml"]);
}

#[test]
fn refuses_paths_that_leave_the_output_folder() {
    let (header, data) = archive(&[
        node(0, "..", None),
        node(0, "evil.txt", Some(b"x".to_vec())),
        node(2, "good.txt", Some(b"y".to_vec())),
    ]);
    let archive = Archive::open_exe(&mut Cursor::new(exe_with(&header))).unwrap();
    let dir = scratch("escape");
    let out = dir.join("out");
    let entries = nefs_dump::selected(&archive, &[]);
    assert_eq!(
        entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["../evil.txt", "good.txt"]
    );
    let err = nefs_dump::dump(&archive, &mut Cursor::new(data), &out, &entries).unwrap_err();
    assert!(err.contains("../evil.txt"), "{err}");
    assert!(!dir.join("evil.txt").exists() && !out.exists());
    assert!(nefs_dump::target(&out, "/etc/passwd").is_none());
    assert!(nefs_dump::target(&out, "").is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reports_usage_without_an_exe_and_an_archive() {
    let mut sink = Vec::new();
    assert_eq!(
        nefs_dump::run(&args(&["F1_25.exe"]), &mut sink).unwrap_err(),
        nefs_dump::USAGE
    );
    assert_eq!(
        nefs_dump::run(&args(&["--help"]), &mut sink).unwrap_err(),
        nefs_dump::USAGE
    );
    assert!(
        nefs_dump::run(&args(&["a", "b", "--out"]), &mut sink)
            .unwrap_err()
            .contains("--out")
    );
    let missing = nefs_dump::run(
        &args(&["/nonexistent/F1_25.exe", "/nonexistent/game.dat"]),
        &mut sink,
    )
    .unwrap_err();
    assert!(missing.contains("/nonexistent/F1_25.exe"), "{missing}");
}
