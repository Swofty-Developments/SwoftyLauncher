//! The requests the launcher sends the Time Trial server, pinned as fixtures in `tests/fixtures/server-contract/`.
//! Each is recorded from the launcher's own code against a stub server, one file per request: its method, path,
//! headers and body, and the stub's answer that the next request builds on. The token, the multipart boundary, the
//! host and the length are left out, the launcher's version and OS are `<launcher>` and `<os>`, and the stub's
//! signatures, made with a key of the test's own, are `<signature>`. The server's tests replay the files against its
//! handlers. `SHA256SUMS` beside them lists each file's sha256, and the server's copy pins the sha256 of that list,
//! so a copy that is not the launcher's fails there. A request that changes shape fails here until its fixture is
//! written again with `SWOFTY_CONTRACT_WRITE=1`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::game_folder::baseline::Identity;
use crate::online::modules;
use crate::online::multiplayer::fingerprint::{AddonPrint, Fingerprint, Source};
use crate::online::server::api::{Api, Boards, TEST_SERVER};
use crate::online::server::integrity;
use crate::online::timetrial::{self, queue::Stamp, swlap, ticks, upload};

const TOKEN: &str = "contract-token";
const NONCE: &str = "5e5510b0c0de5e5510b0c0de5e5510b0";
const BUILD: &str = "46009bd3c0de46009bd3c0de46009bd3c0de46009bd3c0de46009bd3c0de4600";
const WRITE_ENV: &str = "SWOFTY_CONTRACT_WRITE";
const SUMS: &str = "SHA256SUMS";
const TIMED_ID: &str = "2949b7d7eaf78422";
const UNTIMED_ID: &str = "7d0c2a91be55f013";
const TIMED_MS: u64 = 91_234;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server-contract")
}

type Reply = (&'static str, Vec<(&'static str, String)>, Vec<u8>);

struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<Vec<u8>>>>,
    thread: std::thread::JoinHandle<()>,
}

fn stub(replies: Vec<Reply>) -> Stub {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let thread = std::thread::spawn(move || {
        for (status, headers, body) in replies {
            let (mut c, _) = listener.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = c.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                let Some(h) = got.windows(4).position(|w| w == b"\r\n\r\n") else {
                    continue;
                };
                let head = String::from_utf8_lossy(&got[..h]).to_ascii_lowercase();
                let len = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: ")?.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if got.len() >= h + 4 + len {
                    break;
                }
            }
            log.lock().unwrap().push(got);
            let mut head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
            head.push_str("\r\n");
            c.write_all(head.as_bytes()).ok();
            c.write_all(&body).ok();
        }
    });
    Stub { url, seen, thread }
}

impl Stub {
    fn requests(self) -> Vec<Vec<u8>> {
        self.thread.join().unwrap();
        Arc::try_unwrap(self.seen).unwrap().into_inner().unwrap()
    }
}

fn json_reply(status: &'static str, body: &Value) -> Reply {
    (
        status,
        vec![("Content-Type", "application/json".to_owned())],
        body.to_string().into_bytes(),
    )
}

fn placeholders(mut meta: Value) -> Value {
    let launcher = |doc: &mut Value| {
        if let Some(v) = doc.get_mut("launcher") {
            assert_eq!(v.as_str(), Some(env!("CARGO_PKG_VERSION")));
            *v = "<launcher>".into();
        }
    };
    launcher(&mut meta);
    launcher(&mut meta["fingerprint"]);
    if let Some(v) = meta.get_mut("os") {
        assert!(matches!(v.as_str(), Some("windows" | "linux")), "{v}");
        *v = "<os>".into();
    }
    meta
}

fn swlap_header(gz: &[u8]) -> Value {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut raw).unwrap();
    assert!(raw.starts_with(swlap::MAGIC));
    let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
    serde_json::from_slice(&raw[12..12 + len]).unwrap()
}

fn multipart(body: &[u8], boundary: &str) -> Value {
    let text = String::from_utf8_lossy(body).into_owned();
    let mut parts = Vec::new();
    let mark = format!("--{boundary}");
    let mut rest = body;
    while let Some(at) = find(rest, mark.as_bytes()) {
        rest = &rest[at + mark.len()..];
        if rest.starts_with(b"--") {
            break;
        }
        let end = find(rest, mark.as_bytes()).unwrap_or(rest.len());
        let part = &rest[2..end - 2];
        let h = find(part, b"\r\n\r\n").unwrap();
        let head = std::str::from_utf8(&part[..h]).unwrap();
        let data = &part[h + 4..];
        let mut doc = json!({});
        for line in head.split("\r\n") {
            let (name, value) = line.split_once(':').unwrap();
            match name.to_ascii_lowercase().as_str() {
                "content-disposition" => {
                    for field in value.split(';').skip(1) {
                        let (k, v) = field.trim().split_once('=').unwrap();
                        let key = if k == "filename" { "file_name" } else { k };
                        doc[key] = v.trim_matches('"').into();
                    }
                }
                "content-type" => doc["content_type"] = value.trim().into(),
                other => panic!("an unexpected part header {other} in {text}"),
            }
        }
        if doc["content_type"] == "application/json" {
            doc["json"] = placeholders(serde_json::from_slice(data).unwrap());
        } else if doc["file_name"]
            .as_str()
            .is_some_and(|f| f.ends_with(".swlap.gz"))
        {
            doc["gzip_of_swlap"] = json!({"header": swlap_header(data)});
        } else {
            doc["bytes"] = data.len().into();
        }
        parts.push(doc);
    }
    Value::Array(parts)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn shape(raw: &[u8]) -> Value {
    let h = find(raw, b"\r\n\r\n").unwrap();
    let head = std::str::from_utf8(&raw[..h]).unwrap();
    let body = &raw[h + 4..];
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap().split(' ');
    let (method, path) = (first.next().unwrap(), first.next().unwrap());
    let mut headers = BTreeMap::new();
    let mut boundary = None;
    for line in lines {
        let (name, value) = line.split_once(':').unwrap();
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        let value = match name.as_str() {
            "host" | "content-length" => continue,
            "authorization" => {
                assert_eq!(value, format!("Bearer {TOKEN}"));
                "Bearer <token>".to_owned()
            }
            "content-type" if value.starts_with("multipart/form-data; boundary=") => {
                boundary = Some(value["multipart/form-data; boundary=".len()..].to_owned());
                "multipart/form-data; boundary=<boundary>".to_owned()
            }
            _ => value.to_owned(),
        };
        let value = match serde_json::from_str::<Value>(&value) {
            Ok(doc @ Value::Object(_)) => sorted(doc).to_string(),
            _ => value,
        };
        headers.insert(name, Value::String(value));
    }
    let body = if body.is_empty() {
        Value::Null
    } else if let Some(b) = &boundary {
        json!({ "multipart": multipart(body, b) })
    } else if headers.get("content-type").and_then(Value::as_str) == Some("application/json") {
        json!({ "json": serde_json::from_slice::<Value>(body).unwrap() })
    } else {
        json!({ "bytes": body.len() })
    };
    json!({"method": method, "path": path, "headers": headers, "body": body})
}

/// The value with every object's keys in name order, whether or not serde_json keeps insertion order in this build.
fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<(String, Value)> = map.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(pairs.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn unsigned(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if key == "signature" {
                    *v = "<signature>".into();
                } else {
                    unsigned(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(unsigned),
        _ => {}
    }
}

fn answer(reply: &Reply) -> Value {
    let mut body =
        serde_json::from_slice::<Value>(&reply.2).unwrap_or_else(|_| json!({ "bytes": reply.2.len() }));
    unsigned(&mut body);
    let status: u16 = reply.0.split(' ').next().unwrap().parse().unwrap();
    let mut doc = json!({"status": status, "body": body});
    for (name, value) in &reply.1 {
        if !name.eq_ignore_ascii_case("content-type") {
            doc["headers"][name.to_ascii_lowercase()] = value.clone().into();
        }
    }
    doc
}

struct Recorded(Vec<(String, Value)>);

impl Recorded {
    fn add(&mut self, name: &str, about: &str, raw: &[u8], reply: &Reply) {
        assert!(!self.0.iter().any(|(n, _)| n == name), "{name} recorded twice");
        let doc = sorted(json!({"about": about, "request": shape(raw), "answer": answer(reply)}));
        self.0.push((name.to_owned(), doc));
    }
}

fn temp(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("swofty-contract-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (data, game) = (root.join("data"), root.join("game"));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    (data, game)
}

fn signed_in(data: &Path) -> crate::session::Config {
    let path = data.join("multiplayer").join("session.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let session = json!({"token": TOKEN, "name": "driver", "allowed": true});
    std::fs::write(path, session.to_string()).unwrap();
    crate::session::Config {
        time_trial_boards: true,
        time_trial_ingame: true,
        ..Default::default()
    }
}

fn challenge() -> Value {
    json!({
        "v": 1, "build": BUILD, "salt": "c2FsdHNhbHRzYWx0c2FsdHNhbHRzYWx0c2FsdHNhbHQ=",
        "regions": [["text", 4128768, 65536], ["rdata", 1310720, 16384]],
    })
}

fn sessions(out: &mut Recorded) {
    let (data, game) = temp("sessions");
    let config = signed_in(&data);
    let issued = json_reply(
        "200 OK",
        &json!({"nonce": NONCE, "expires": 1_795_010_000u64, "challenge": challenge()}),
    );
    let old = json_reply("200 OK", &json!({"nonce": NONCE, "expires": 1_795_010_000u64}));
    let s = stub(vec![issued.clone(), old.clone(), issued.clone()]);
    TEST_SERVER.with(|t| *t.borrow_mut() = Some(s.url.clone()));
    let n = upload::nonce(&data, &config, Some(BUILD)).unwrap();
    assert_eq!(n.challenge, Some(challenge()));
    let n = upload::nonce(&data, &config, None).unwrap();
    assert_eq!(n.challenge, None);
    upload::nonce_until_issued(&data, &game, &config, Some(BUILD), &|| false, &|_| {});
    TEST_SERVER.with(|t| *t.borrow_mut() = None);
    let session: Value =
        serde_json::from_slice(&std::fs::read(game.join("swofty").join("tt.json")).unwrap()).unwrap();
    assert_eq!(
        (&session["tt_nonce"], &session["tt_challenge"]),
        (&json!(NONCE), &challenge())
    );
    let seen = s.requests();
    out.add(
        "session-start",
        "The session nonce asked for at game start, with the game build (the exe's sha256). The answer's challenge \
         goes into the game's swofty/tt.json as tt_challenge.",
        &seen[0],
        &issued,
    );
    out.add(
        "session-start-no-build",
        "The session nonce asked for when the launcher does not know the game build, as 1.9.x launchers always ask.",
        &seen[1],
        &old,
    );
    out.add(
        "session-retry",
        "The session nonce asked for again while the game runs, after the start could not get one.",
        &seen[2],
        &issued,
    );
}

fn receipt(mark: &str) -> String {
    format!(
        "tk1.{}.bWFj",
        integrity::tests::b64(mark.as_bytes()).trim_end_matches('=')
    )
}

fn ticks(out: &mut Recorded, data: &Path, game: &Path) {
    let folder = ticks::folders(data, game)[0].clone();
    std::fs::create_dir_all(&folder).unwrap();
    let ok = |mark: &str| json_reply("200 OK", &json!({ "receipt": receipt(mark) }));
    let busy = json_reply("503 Service Unavailable", &json!({"error": "the server is busy"}));
    let replies = vec![
        ok("start"),
        busy.clone(),
        ok("s1"),
        ok("p1"),
        ok("r1"),
        ok("s2"),
        ok("end"),
    ];
    let s = stub(replies.clone());
    let api = Api::with_server(&s.url, TOKEN);
    let mut sender = ticks::Sender::new(data, ticks::folders(data, game));
    let t0 = timetrial::queue::unix_now() * 1000 - 120_000;
    let marks = [
        ("start", 0u64, 0u64, 12u64),
        ("s1", 30_000, 28_411, 9),
        ("p1", 40_000, 38_200, 8),
        ("r1", 46_000, 38_200, 10),
        ("s2", 61_000, 59_020, 7),
        ("end", 91_234, 91_234, 11),
    ];
    for (seq, (mark, at, game_ms, late)) in marks.into_iter().enumerate() {
        let at = t0 + at;
        let body = json!({"seq": seq + 1, "lap": 1, "mark": mark, "game_ms": game_ms, "at_ms": at});
        std::fs::write(folder.join(format!("{at}-{}.json", seq + 1)), body.to_string()).unwrap();
        sender.step(&api, Some(NONCE), at + late, &|_| {});
        if mark == "s1" {
            sender.step(&api, Some(NONCE), at + late + 600, &|_| {});
        }
    }
    assert!(!ticks::pending(data, game));
    let seen = s.requests();
    let about = |mark: &str| {
        format!(
            "The {mark} timing mark of lap 1, sent as it is made, chained to the receipt of the lap's previous mark."
        )
    };
    out.add("tick-start", &about("start"), &seen[0], &replies[0]);
    out.add("tick-s1", &about("s1"), &seen[1], &replies[1]);
    out.add(
        "tick-s1-retry",
        "The s1 mark sent again after the server answered 503: the same body with attempt 2 and since_ms updated.",
        &seen[2],
        &replies[2],
    );
    out.add(
        "tick-p1",
        "The p1 mark of lap 1, as the game pauses, chained to the receipt of the lap's start; the game's lap clock stands still, and a pause that can't be sent is dropped alone.",
        &seen[3],
        &replies[3],
    );
    out.add(
        "tick-r1",
        "The r1 mark of lap 1, as the game resumes, chained to the receipt of the p1 mark.",
        &seen[4],
        &replies[4],
    );
    out.add("tick-s2", &about("s2"), &seen[5], &replies[5]);
    out.add("tick-end", &about("end"), &seen[6], &replies[6]);
}

fn lap(time_ms: u64, lap_index: u32) -> Vec<u8> {
    let header = json!({
        "format": 1,
        "core": "1.10.0",
        "nonce": NONCE,
        "board": {"content": "game/f1_2025", "track": "bahrain", "weather": "dry"},
        "lap": {"time_ms": time_ms, "valid": true, "sectors_ms": [28_411, 30_609, 32_214], "lap_index": lap_index},
    })
    .to_string();
    let mut out = swlap::MAGIC.to_vec();
    out.extend((header.len() as u32).to_le_bytes());
    out.extend(header.as_bytes());
    out.extend(b"END\0\0\0\0\0");
    out
}

fn fingerprint() -> Value {
    let print = |id: &str, source: Source| AddonPrint {
        id: id.into(),
        name: id.into(),
        version: semver::Version::new(1, 10, 0),
        source,
        registry: None,
        inputs: None,
        order: None,
        detail: None,
        stat_ids: BTreeMap::new(),
    };
    let fp = Fingerprint::new(
        Identity {
            build_id: "26566308".into(),
            exe_sha256: BUILD.into(),
        },
        None,
        vec![
            print(crate::addons::builtin::CORE_ID, Source::Builtin("core".into())),
            print("swofty.std", Source::Builtin("std".into())),
        ],
        BTreeMap::new(),
        None,
    );
    timetrial::fingerprint::wire(&fp)
}

fn module(source: integrity::Source) -> integrity::Module {
    integrity::Module {
        version: "2026.11.18.1".into(),
        dll: PathBuf::from("swofty_integrity.dll"),
        sha256: String::new(),
        fallback: None,
        source,
    }
}

fn uploads(out: &mut Recorded, data: &Path, game: &Path) {
    let laps = timetrial::laps_dirs(data, game)[0].clone();
    std::fs::create_dir_all(&laps).unwrap();
    let timed = json_reply(
        "200 OK",
        &json!({"id": TIMED_ID, "status": "accepted", "rank": 3, "total": 40, "best": true}),
    );
    let untimed = json_reply(
        "200 OK",
        &json!({"id": UNTIMED_ID, "status": "accepted", "rank": 5, "total": 40, "best": false}),
    );
    let s = stub(vec![timed.clone(), untimed.clone()]);
    let api = Api::with_server(&s.url, TOKEN);
    let sweep = |name: &str, time_ms: u64, lap_index: u32, state: integrity::State| {
        std::fs::write(laps.join(name), lap(time_ms, lap_index)).unwrap();
        let stamp = Stamp {
            fingerprint: fingerprint(),
            integrity: serde_json::to_value(state).unwrap(),
        };
        let r = upload::sweep(
            data,
            Some(game),
            Some(&api as &dyn Boards),
            Some(&stamp),
            None,
            &mut |_| {},
        );
        assert_eq!(r.uploaded, 1);
    };
    sweep(
        "20261118T000001-1.swlap",
        TIMED_MS,
        1,
        integrity::State::of(&Ok(module(integrity::Source::Cdn)), false),
    );
    sweep(
        "20261118T000002-1.swlap",
        90_870,
        3,
        integrity::State::of(
            &Err(integrity::Unavailable::unreachable(
                "couldn't reach the server: timeout: global",
            )),
            false,
        ),
    );
    let seen = s.requests();
    out.add(
        "lap-upload-timed",
        "A sealed lap's upload: the module came from the CDN, and lap 1 carries the receipts of its four timing marks.",
        &seen[0],
        &timed,
    );
    out.add(
        "lap-upload-untimed",
        "A lap from a session that placed no integrity module, driven with no timing marks sent: no timing. It is lap 3: \
         the plugin discarded lap 2, which keeps its number, so lap numbers have gaps.",
        &seen[1],
        &untimed,
    );
}

fn offers(out: &mut Recorded) {
    let key = integrity::tests::pair();
    let bytes = b"MZ contract module".to_vec();
    let offered = integrity::tests::offer(&key, "2026.11.18.1", &bytes);
    let offer = json_reply("200 OK", &offered);
    let file: Reply = (
        "200 OK",
        vec![("Content-Type", "application/octet-stream".to_owned())],
        bytes.clone(),
    );
    let s = stub(vec![offer.clone(), file.clone()]);
    let (data, _) = temp("integrity");
    use ring::signature::KeyPair;
    integrity::ensure_with(&data, &Api::with_server(&s.url, TOKEN), key.public_key().as_ref()).unwrap();
    let seen = s.requests();
    out.add(
        "integrity-offer",
        "The integrity module offer, asked for before each signed-in session.",
        &seen[0],
        &offer,
    );
    out.add(
        "integrity-file",
        "The integrity module's file, when the offered version is not cached and the CDN copy can't be had.",
        &seen[1],
        &file,
    );

    let listed = modules::tests::listing(&[modules::tests::offer_of(
        &key,
        "2026.11.18.2",
        modules::TT_GAME.abi,
        &bytes,
    )]);
    let listing: Reply = (
        "200 OK",
        vec![("Content-Type", "application/json".to_owned())],
        listed,
    );
    let s = stub(vec![listing.clone(), file.clone()]);
    let (data, _) = temp("modules");
    modules::ensure_with(
        &data,
        &Api::with_server(&s.url, TOKEN),
        modules::TT_GAME.name,
        modules::GAME_PLATFORM,
        modules::TT_GAME.abi,
        key.public_key().as_ref(),
    )
    .unwrap();
    let seen = s.requests();
    out.add(
        "module-offer",
        "The Time Trial game module listing, asked for when a signed-in session builds.",
        &seen[0],
        &listing,
    );
    out.add(
        "module-file",
        "The Time Trial game module's file.",
        &seen[1],
        &file,
    );
}

fn lap_meta(id: &str) -> Value {
    json!({
        "id": id,
        "time_ms": TIMED_MS,
        "board": {"content": "game/f1_2025", "track": "bahrain", "weather": "dry"},
        "user": {"id": "1", "name": "driver one"},
        "car": {"team": "mclaren"},
        "game_replay": {"setup": {"mode": "tt", "box": "f1_2025", "track": "bahrain", "team": "mclaren", "lap_ms": TIMED_MS, "tail_ms": 1500}},
    })
}

fn replays(out: &mut Recorded, data: &Path, game: &Path) {
    let replay: Vec<u8> = b"FRR1"
        .iter()
        .copied()
        .chain((0..8188u32).map(|i| i as u8))
        .collect();
    let setup = lap_meta(TIMED_ID)["game_replay"]["setup"].clone();
    let dir = timetrial::game_replay::dir(game);
    std::fs::create_dir_all(&dir).unwrap();
    let now = timetrial::queue::unix_now();
    let file = dir.join(format!("pb_{}_{TIMED_MS}.replay", now - 5));
    std::fs::write(&file, &replay).unwrap();
    std::fs::write(file.with_extension("json"), setup.to_string()).unwrap();
    let stored = json_reply("200 OK", &json!({"ok": true}));
    let s = stub(vec![stored.clone()]);
    let api = Api::with_server(&s.url, TOKEN);
    let queue = timetrial::queue::Queue::new(data);
    let r = timetrial::game_replay::send(
        &queue,
        &timetrial::game_replay::dirs(data, game),
        &api,
        now + 10,
        &|| false,
        &mut |_| {},
    );
    assert_eq!(r.uploaded, 1);
    let seen = s.requests();
    out.add(
        "game-replay-put",
        "The game's replay of the driver's best lap, uploaded once the server has taken the lap, with its setup.",
        &seen[0],
        &stored,
    );

    let meta = json_reply("200 OK", &lap_meta(TIMED_ID));
    let file: Reply = (
        "200 OK",
        vec![("Content-Type", "application/octet-stream".to_owned())],
        replay.clone(),
    );
    let s = stub(vec![meta.clone(), file.clone()]);
    timetrial::game_replay::add(&Api::with_server(&s.url, TOKEN), data, TIMED_ID).unwrap();
    let seen = s.requests();
    out.add(
        "lap-read",
        "A lap's details, read before its game replay or its lap file is fetched.",
        &seen[0],
        &meta,
    );
    out.add(
        "game-replay-get",
        "A lap's game replay, fetched to watch it in the game.",
        &seen[1],
        &file,
    );

    let lap_file: Reply = (
        "200 OK",
        vec![("Content-Type", "application/octet-stream".to_owned())],
        lap(TIMED_MS, 1),
    );
    let s = stub(vec![meta.clone(), lap_file.clone()]);
    timetrial::watch::request(&Api::with_server(&s.url, TOKEN), game, TIMED_ID).unwrap();
    let seen = s.requests();
    assert_eq!(
        out.0.iter().find(|(n, _)| n == "lap-read").unwrap().1["request"],
        shape(&seen[0])
    );
    out.add(
        "lap-file-get",
        "A lap's file, fetched to race it as a ghost.",
        &seen[1],
        &lap_file,
    );
}

fn boards(out: &mut Recorded) {
    let catalog = json_reply(
        "200 OK",
        &json!({"rows": [{"content": "game/f1_2025", "name": "F1 25", "tracks": [{"track": "bahrain", "weather": "dry", "laps": 40}]}]}),
    );
    let board = json_reply(
        "200 OK",
        &json!({"rows": [{"rank": 1, "lap_id": TIMED_ID, "time_ms": TIMED_MS, "driver": {"id": "1", "name": "driver one"}}]}),
    );
    let barred = json_reply("200 OK", &json!({"addons": []}));
    let s = stub(vec![catalog.clone(), board.clone(), barred.clone()]);
    let api = Api::with_server(&s.url, TOKEN);
    assert_eq!(api.catalog().unwrap().len(), 1);
    assert_eq!(
        api.board("game/f1_2025", "bahrain", "dry", Some("driver one"))
            .unwrap()
            .len(),
        1
    );
    timetrial::barred::fetch(&s.url).unwrap();
    let seen = s.requests();
    out.add("boards", "The list of Time Trial boards.", &seen[0], &catalog);
    out.add(
        "board",
        "One board, searched for a driver's name.",
        &seen[1],
        &board,
    );
    out.add(
        "barred",
        "The addons barred from the boards, read without the sign-in.",
        &seen[2],
        &barred,
    );
}

fn record() -> Recorded {
    let mut out = Recorded(Vec::new());
    sessions(&mut out);
    let (data, game) = temp("laps");
    ticks(&mut out, &data, &game);
    uploads(&mut out, &data, &game);
    replays(&mut out, &data, &game);
    boards(&mut out);
    offers(&mut out);
    out
}

fn text(doc: &Value) -> String {
    serde_json::to_string_pretty(doc).unwrap() + "\n"
}

/// `<sha256>  <file>` for each fixture, in name order: what the server's copy is checked against.
fn sums(recorded: &Recorded) -> String {
    let mut lines: Vec<String> = recorded
        .0
        .iter()
        .map(|(name, doc)| {
            format!(
                "{}  {name}.json\n",
                crate::game_folder::baseline::sha256_bytes(text(doc).as_bytes())
            )
        })
        .collect();
    lines.sort_by(|a, b| a[66..].cmp(&b[66..]));
    lines.concat()
}

#[test]
fn every_request_to_the_server_matches_its_contract_fixture() {
    let recorded = record();
    let dir = fixtures();
    if std::env::var_os(WRITE_ENV).is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            if entry.path().extension().is_some_and(|x| x == "json") {
                std::fs::remove_file(entry.path()).unwrap();
            }
        }
        for (name, doc) in &recorded.0 {
            std::fs::write(dir.join(format!("{name}.json")), text(doc)).unwrap();
        }
        std::fs::write(dir.join(SUMS), sums(&recorded)).unwrap();
    }
    let mut wrong = Vec::new();
    for (name, doc) in &recorded.0 {
        let path = dir.join(format!("{name}.json"));
        let kept = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        if kept.as_ref() != Some(doc) {
            wrong.push(format!(
                "{name}: the launcher now sends\n{}\nbut {} has\n{}",
                serde_json::to_string_pretty(doc).unwrap(),
                path.display(),
                kept.map_or("nothing".into(), |k| serde_json::to_string_pretty(&k).unwrap())
            ));
        }
    }
    if std::fs::read_to_string(dir.join(SUMS)).ok().as_deref() != Some(sums(&recorded).as_str()) {
        wrong.push(format!(
            "{SUMS}: it is not the sha256 of each fixture as the launcher writes it"
        ));
    }
    let names: Vec<String> = recorded.0.iter().map(|(n, _)| format!("{n}.json")).collect();
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.ends_with(".json") && !names.contains(&file) {
            wrong.push(format!(
                "{file}: no request of the launcher has this shape any more"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "a request to the server changed shape. Update the server's contract test with it, then write the fixtures \
         again with {WRITE_ENV}=1:\n\n{}",
        wrong.join("\n\n")
    );
}
