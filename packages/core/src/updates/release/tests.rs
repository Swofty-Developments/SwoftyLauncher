use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;

use super::download::*;
use super::fetch::*;
use super::install::*;
use super::unpack::*;
use super::*;
use std::io::Write;

use sha2::{Digest, Sha256};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-update-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn release_tree(root: &Path, tag: &str) {
    write(
        &root.join("bin").join(exe("swofty-launcher")),
        &format!("launcher {tag}"),
    );
    write(&root.join("bin").join(exe("swofty")), &format!("cli {tag}"));
    write(
        &root.join("loader").join(crate::install::PROXY),
        &format!("proxy {tag}"),
    );
    write(
        &root.join("loader").join(crate::addons::builtin::CORE_DLL),
        &format!("core {tag}"),
    );
}

fn asset(name: &str, size: u64, digest: Option<&str>) -> Asset {
    Asset {
        name: name.into(),
        size,
        browser_download_url: format!("https://example.invalid/{name}"),
        digest: digest.map(Into::into),
    }
}

#[test]
fn resumes_a_release_download_the_connection_cut_off() {
    let body = b"launcher bytes ".repeat(4000);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut a = asset("swofty-launcher.zip", body.len() as u64, None);
    a.browser_download_url = format!("http://{}/a.zip", listener.local_addr().unwrap());
    let half = body.len() / 2;
    let served = body.clone();
    let server = std::thread::spawn(move || {
        let mut heads = Vec::new();
        for first in [true, false] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            heads.push(String::from_utf8_lossy(&head).to_ascii_lowercase());
            let answer = if first {
                let mut out = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    served.len()
                )
                .into_bytes();
                out.extend(&served[..half]);
                out
            } else {
                let mut out = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {half}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        served.len() - 1,
                        served.len(),
                        served.len() - half
                    )
                    .into_bytes();
                out.extend(&served[half..]);
                out
            };
            stream.write_all(&answer).unwrap();
        }
        heads
    });
    let dir = temp("resume");
    let to = dir.join("a.zip");
    let (len, sha256) = download(&a, &to, &|_| {}).unwrap();
    assert_eq!(len, body.len() as u64);
    assert_eq!(std::fs::read(&to).unwrap(), body);
    let whole: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(sha256, whole);
    let heads = server.join().unwrap();
    assert!(!heads[0].contains("range:"), "{}", heads[0]);
    assert!(
        heads[1].contains(&format!("range: bytes={half}-")),
        "{}",
        heads[1]
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn compares_versions_by_semver() {
    assert_eq!(parse_version("v0.2.1"), Some(v("0.2.1")));
    assert_eq!(parse_version("0.10.0"), Some(v("0.10.0")));
    assert_eq!(parse_version("latest"), None);
    assert!(parse_version("v0.10.0").unwrap() > v("0.9.9"));
    assert!(parse_version("v0.2.1-rc.1").unwrap() < v("0.2.1"));
}

#[test]
fn reads_rate_limits_from_the_response() {
    let url = "https://api.github.com/x";
    assert!(matches!(
        refusal(url, 403, Some("120"), None, None, 1000),
        Failure::Wait(120, _)
    ));
    assert!(matches!(
        refusal(url, 403, None, Some("0"), Some("1600"), 1000),
        Failure::Wait(600, _)
    ));
    assert!(matches!(
        refusal(url, 403, None, Some("0"), Some("900"), 1000),
        Failure::Wait(1, _)
    ));
    assert!(matches!(
        refusal(url, 429, None, None, None, 1000),
        Failure::Wait(60, _)
    ));
    assert!(matches!(
        refusal(url, 403, None, Some("12"), Some("1600"), 1000),
        Failure::Failed(_)
    ));
    assert_eq!(
        refusal(url, 404, None, None, None, 0),
        Failure::Failed(format!("{url}: HTTP 404"))
    );
    let text = refusal(url, 403, None, Some("0"), Some("1600"), 1000).to_string();
    assert!(
        text.contains("rate limit") && text.contains("10 minutes"),
        "{text}"
    );
}

fn manifest_json(tag: &str, asset_sha: &str) -> String {
    let linux = format!("swofty-launcher-{tag}-linux-x86_64.tar.gz");
    let windows = format!("swofty-launcher-{tag}-windows-x86_64.zip");
    format!(
        r#"{{"schema":1,"version":"{v}","tag":"{tag}","published_at":"2026-11-08T09:16:53Z","notes_url":"https://example.test/notes","assets":[
{{"name":"{linux}","url":"https://cdn.swofty.net/launcher/{tag}/{linux}","size":10,"sha256":"{asset_sha}"}},
{{"name":"{windows}","url":"https://cdn.swofty.net/launcher/{tag}/{windows}","size":20,"sha256":"{asset_sha}"}},
{{"name":"SHA256SUMS","url":"https://cdn.swofty.net/launcher/{tag}/SHA256SUMS","size":3,"sha256":"{asset_sha}"}}]}}"#,
        v = tag.trim_start_matches('v')
    )
}

fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let n = stream.read(&mut request).unwrap_or(0);
            let head = String::from_utf8_lossy(&request[..n]).into_owned();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
            let (status, body) = routes
                .iter()
                .find(|(p, _, _)| *p == path)
                .map_or((404, Vec::new()), |(_, s, b)| (*s, b.clone()));
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://{addr}")
}

fn github_json(tag: &str) -> Vec<u8> {
    format!(r#"{{"tag_name":"{tag}","html_url":"https://github.test/{tag}","assets":[]}}"#).into_bytes()
}

#[test]
fn reads_the_cdn_manifest_into_a_release() {
    let sha = "A".repeat(64);
    let manifest: Manifest = serde_json::from_str(&manifest_json("v1.8.29", &sha)).unwrap();
    let release = manifest.release().unwrap();
    assert_eq!(release.tag_name, "v1.8.29");
    assert_eq!(release.notes_url(), "https://example.test/notes");
    assert!(release.sums().is_none());
    assert_eq!(release.assets.len(), 2);
    let linux = release.asset("linux", "x86_64").unwrap();
    assert_eq!(linux.size, 10);
    assert_eq!(
        linux.browser_download_url,
        "https://cdn.swofty.net/launcher/v1.8.29/swofty-launcher-v1.8.29-linux-x86_64.tar.gz"
    );
    assert_eq!(
        linux.digest.as_deref(),
        Some(format!("sha256:{}", "a".repeat(64)).as_str())
    );
    assert_eq!(release.asset("windows", "x86_64").unwrap().size, 20);
    assert!(release.asset("macos", "aarch64").is_none());
    assert!(verifiable(&release, linux));
}

#[test]
fn rejects_an_invalid_manifest() {
    let sha = "a".repeat(64);
    let good = manifest_json("v1.8.29", &sha);
    let parse = |text: &str| serde_json::from_str::<Manifest>(text).unwrap().release();
    assert!(parse(&good).is_ok());
    assert!(parse(&good.replace(r#""schema":1"#, r#""schema":2"#)).is_err());
    assert!(parse(&good.replace(r#""version":"1.8.29""#, r#""version":"1.8.28""#)).is_err());
    assert!(parse(&good.replace(r#""tag":"v1.8.29""#, r#""tag":"nightly""#)).is_err());
    assert!(parse(&good.replace(&sha, "abc")).is_err());
    assert!(parse(&good.replace("swofty-launcher-v1.8.29-linux", "../x-linux")).is_err());
    assert!(parse(&good.replace(r#""size":10"#, r#""size":0"#)).is_err());
    let sums_only = r#"{"schema":1,"version":"1.0.0","tag":"v1.0.0","assets":[{"name":"SHA256SUMS","url":"u","size":1,"sha256":""}]}"#;
    assert!(parse(sums_only).is_err());
    assert!(serde_json::from_str::<Manifest>("<html>").is_err());
}

#[test]
fn rejects_a_download_that_does_not_match_the_manifest() {
    let sha = "a".repeat(64);
    let release = serde_json::from_str::<Manifest>(&manifest_json("v1.8.29", &sha))
        .unwrap()
        .release()
        .unwrap();
    let asset = release.asset("linux", "x86_64").unwrap();
    assert!(check_download(asset, 10, &sha, None).is_ok());
    let err = check_download(asset, 10, &"b".repeat(64), None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("SHA-256") && err.contains(&sha), "{err}");
    assert!(check_download(asset, 9, &sha, None).is_err());
}

#[test]
fn takes_the_release_from_the_cdn_when_its_manifest_is_good() {
    let sha = "c".repeat(64);
    let cdn = serve(vec![(
        "/launcher/latest.json",
        200,
        manifest_json("v2.0.0", &sha).into_bytes(),
    )]);
    let github = serve(vec![("/latest", 200, github_json("v1.0.0"))]);
    let release = fetch_latest_from(
        &format!("{cdn}/launcher/latest.json"),
        &format!("{github}/latest"),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(release.tag_name, "v2.0.0");
}

#[test]
fn falls_back_to_github_when_the_cdn_has_no_usable_manifest() {
    let github = serve(vec![("/latest", 200, github_json("v1.0.0"))]);
    let sha = "c".repeat(64);
    let broken = [
        ("missing", 404, Vec::new()),
        ("html", 200, b"<html>oops</html>".to_vec()),
        (
            "wrong-schema",
            200,
            manifest_json("v2.0.0", &sha)
                .replace(r#""schema":1"#, r#""schema":9"#)
                .into_bytes(),
        ),
        (
            "empty",
            200,
            br#"{"schema":1,"version":"2.0.0","tag":"v2.0.0","assets":[]}"#.to_vec(),
        ),
    ];
    for (name, status, body) in broken {
        let cdn = serve(vec![("/latest.json", status, body)]);
        let release = fetch_latest_from(
            &format!("{cdn}/latest.json"),
            &format!("{github}/latest"),
            Duration::from_secs(5),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(release.tag_name, "v1.0.0", "{name}");
    }
}

#[test]
fn reports_both_failures_when_the_cdn_and_github_fail() {
    let cdn = serve(vec![]);
    let github = serve(vec![]);
    let err = fetch_latest_from(
        &format!("{cdn}/latest.json"),
        &format!("{github}/latest"),
        Duration::from_secs(5),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("/latest: HTTP 404") && err.contains("latest.json: HTTP 404"),
        "{err}"
    );
}

#[test]
fn downloads_from_github_when_the_cdn_copy_is_missing() {
    use sha2::{Digest, Sha256};
    let body = b"0123456789".to_vec();
    let sha: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    let github = serve(vec![("/asset", 200, body.clone())]);
    let cdn = serve(vec![]);
    let asset = Asset {
        name: "a.tar.gz".into(),
        size: 10,
        browser_download_url: format!("{cdn}/asset"),
        digest: Some(format!("sha256:{sha}")),
    };
    let to = temp("mirror").join("a.tar.gz");
    let (len, got) = download_or(&asset, Some(&format!("{github}/asset")), &to, &|_| {}).unwrap();
    assert_eq!((len, got.as_str()), (10, sha.as_str()));
    assert!(check_download(&asset, len, &got, None).is_ok());
    assert!(download_or(&asset, None, &to, &|_| {}).is_err());
}

#[test]
fn only_cdn_downloads_have_a_github_mirror() {
    let release = Release {
        tag_name: "v1.8.29".into(),
        ..Default::default()
    };
    let on = |url: &str| Asset {
        name: "x.zip".into(),
        size: 1,
        browser_download_url: url.into(),
        digest: None,
    };
    assert_eq!(
        mirror_url(&release, &on("https://cdn.swofty.net/launcher/v1.8.29/x.zip")).as_deref(),
        Some("https://github.com/Swofty-Developments/SwoftyLauncher/releases/download/v1.8.29/x.zip")
    );
    assert!(
        mirror_url(
            &release,
            &on("https://github.com/o/r/releases/download/v1.8.29/x.zip")
        )
        .is_none()
    );
}

#[test]
#[ignore = "reads the live https://cdn.swofty.net/launcher/latest.json and downloads one asset"]
fn live_cdn_serves_a_verifiable_release() {
    let release = fetch_manifest(MANIFEST_URL, Duration::from_secs(30)).unwrap();
    let asset = release.platform_asset().unwrap();
    assert!(asset.browser_download_url.starts_with(CDN_HOST));
    let to = temp("live").join(&asset.name);
    let (len, sha) = download(asset, &to, &|_| {}).unwrap();
    check_download(asset, len, &sha, None).unwrap();
    println!("{} {} bytes sha256 {sha}", asset.name, len);
}

#[test]
fn a_release_offers_only_what_it_can_verify() {
    let name = "swofty-launcher-v0.2.1-linux-x86_64.tar.gz";
    let mut release = Release {
        tag_name: "v0.2.1".into(),
        html_url: String::new(),
        assets: vec![asset(name, 1, None)],
        ..Default::default()
    };
    assert!(!verifiable(&release, &release.assets[0]));
    release.assets[0].digest = Some(format!("sha256:{}", "a".repeat(64)));
    assert!(verifiable(&release, &release.assets[0]));
    release.assets[0].digest = None;
    release.assets.push(asset(SUMS_FILE, 3, None));
    assert!(verifiable(&release, &release.assets[0]));
}

#[test]
fn selects_the_asset_for_each_platform() {
    let tag = "v0.2.1";
    let release = Release {
        tag_name: tag.into(),
        html_url: String::new(),
        assets: vec![
            asset("swofty-launcher-v0.2.1-linux-x86_64.tar.gz", 1, None),
            asset("swofty-launcher-v0.2.1-windows-x86_64.zip", 2, None),
            asset(SUMS_FILE, 3, None),
        ],
        ..Default::default()
    };
    assert_eq!(release.asset("linux", "x86_64").unwrap().size, 1);
    assert_eq!(release.asset("windows", "x86_64").unwrap().size, 2);
    assert!(release.asset("macos", "aarch64").is_none());
    assert!(release.asset("linux", "aarch64").is_none());
    assert_eq!(release.sums().unwrap().size, 3);
    assert_eq!(release.notes_url(), format!("{RELEASES_URL}/tag/{tag}"));
}

#[test]
fn parses_sha256sums() {
    let a = "a".repeat(64);
    let b = "B".repeat(64);
    let sums = parse_sums(&format!(
        "{a}  swofty-launcher-v0.2.1-linux-x86_64.tar.gz\n{b} *swofty-launcher-v0.2.1-windows-x86_64.zip\nnot a line\n\n"
    ));
    assert_eq!(sums.len(), 2);
    assert_eq!(sums["swofty-launcher-v0.2.1-linux-x86_64.tar.gz"], a);
    assert_eq!(sums["swofty-launcher-v0.2.1-windows-x86_64.zip"], "b".repeat(64));
}

#[test]
fn verifies_downloads_against_sums_digest_or_size() {
    let good = "c".repeat(64);
    let bad = "d".repeat(64);
    let a = asset("x.zip", 10, Some(&format!("sha256:{good}")));
    let sums = BTreeMap::from([("x.zip".to_owned(), good.clone())]);
    assert!(check_download(&a, 10, &good, Some(&sums)).is_ok());
    assert!(check_download(&a, 10, &bad, Some(&sums)).is_err());
    assert!(check_download(&a, 9, &good, Some(&sums)).is_err());
    assert!(check_download(&a, 10, &good, Some(&BTreeMap::new())).is_err());
    assert!(check_download(&a, 10, &good, None).is_ok());
    assert!(check_download(&a, 10, &bad, None).is_err());
    let size_only = asset("x.zip", 10, None);
    assert!(check_download(&size_only, 10, &bad, None).is_ok());
    assert!(check_download(&size_only, 11, &bad, None).is_err());
}

#[test]
fn locates_only_release_layouts() {
    let dir = temp("locate");
    release_tree(&dir, "v1");
    let launcher = dir.join("bin").join(exe("swofty-launcher"));
    assert_eq!(Install::locate(&launcher).unwrap().root, dir);
    assert!(Install::locate(&dir.join("target/debug/swofty-launcher")).is_err());
    std::fs::remove_dir_all(dir.join("loader")).unwrap();
    assert!(Install::locate(&launcher).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_install_that_holds_the_data_folder_cannot_update() {
    let dir = temp("apart");
    release_tree(&dir, "v1");
    let launcher = dir.join("bin").join(exe("swofty-launcher"));
    let install = || Install::locate(&launcher).unwrap();
    assert_eq!(
        install().apart_from(&dir.join("Swofty")).unwrap_err(),
        DATA_IN_INSTALL
    );
    assert!(install().apart_from(&dir).is_err());
    let outside = temp("apart-data");
    assert!(install().apart_from(&outside).is_ok());
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn extracts_both_archive_kinds() {
    let dir = temp("extract");
    let tree = dir.join("swofty-launcher-v2-x");
    release_tree(&tree, "v2");
    let tgz = dir.join("a.tar.gz");
    {
        let gz =
            flate2::write::GzEncoder::new(std::fs::File::create(&tgz).unwrap(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        b.append_dir_all("swofty-launcher-v2-x", &tree).unwrap();
        b.into_inner().unwrap().finish().unwrap();
    }
    let zip_path = dir.join("a.zip");
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        for (rel, path) in crate::addons::builtin::loader_files(&tree).unwrap() {
            z.start_file(
                format!("swofty-launcher-v2-x\\{}", rel.replace('/', "\\")),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            z.write_all(&std::fs::read(path).unwrap()).unwrap();
        }
        z.finish().unwrap();
    }
    for archive in [&tgz, &zip_path] {
        let out = dir.join(format!("out-{}", archive.extension().unwrap().to_string_lossy()));
        extract(archive, &out).unwrap();
        let root = release_root(&out).unwrap();
        assert_eq!(root, out.join("swofty-launcher-v2-x"));
        assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v2");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[cfg(windows)]
fn replaces_a_running_program() {
    let dir = temp("running");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    let running = root.join("bin").join(exe("swofty-launcher"));
    let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/PING.EXE");
    std::fs::copy(ping, &running).unwrap();
    let mut child = std::process::Command::new(&running)
        .args(["-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    release_tree(&new, "v2");
    let result = apply(&new, &root, &dir.join("old"));
    child.kill().ok();
    child.wait().ok();
    result.unwrap();
    assert_eq!(read(&running), "launcher v2");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_install_root_that_is_or_holds_the_data_folder_is_refused() {
    let dir = temp("refuse");
    let data = dir.join("Swofty");
    std::fs::create_dir_all(data.join("addons")).unwrap();
    assert!(refuse_data_root(&data, &data).is_err());
    assert!(refuse_data_root(&dir, &data).is_err());
    assert!(refuse_data_root(&dir.join("elsewhere"), &data).is_ok());
    assert!(refuse_data_root(&data.join("app"), &data).is_ok());
    let moved = dir.join("D");
    write(&moved.join("saves").join("p").join("career.sav"), "career");
    let stale_config = dir.join("C-old");
    assert!(refuse_data_root(&moved, &stale_config).is_err());
    let custom = dir.join("install").join("mydata");
    std::fs::create_dir_all(&custom).unwrap();
    assert!(refuse_data_root(&dir.join("install"), &custom).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_refused_update_names_the_folder_the_data_link_resolves_to() {
    use crate::data::relocate::{link, real_path};
    let dir = temp("refuse-link");
    let install = dir.join("E").join("Modowanie F1");
    release_tree(&install, "v1");
    write(&install.join("config.json"), "{}");
    let default = dir.join("Local").join("Swofty");
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    link::create(&install, &default).unwrap();
    let real = real_path(&install).display().to_string();
    let text = refuse_data_root(&install, &default).unwrap_err().to_string();
    assert!(
        text.contains(&format!(
            "the install folder {real} is also the data folder ({} links to it)",
            default.display()
        )),
        "{text}"
    );
    assert!(text.contains("new, empty folder"), "{text}");
    assert!(!text.contains("is or contains"), "{text}");
    let inside = install.join("Swofty");
    std::fs::create_dir_all(&inside).unwrap();
    link::remove(&default).unwrap();
    link::create(&inside, &default).unwrap();
    let text = refuse_data_root(&install, &default).unwrap_err().to_string();
    assert!(
        text.contains(&format!(
            "the data folder {} ({} links to it) is inside the install folder {real}",
            real_path(&inside).display(),
            default.display()
        )),
        "{text}"
    );
    let plain = text.replace(&default.display().to_string(), "");
    assert!(!plain.contains(r"\\?\"), "{text}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn user_addons_survive_when_the_install_root_is_the_data_folder() {
    let dir = temp("data-root");
    let (root, new) = (dir.join("Swofty"), dir.join("new"));
    release_tree(&root, "v1");
    write(&root.join("addons").join("mine").join("addon.json"), "{}");
    write(&root.join("addons").join("other.swoftyaddon"), "user");
    write(&root.join("loader").join("stale.dll"), "stale");
    write(&root.join("saves").join("p").join("career.sav"), "career");
    release_tree(&new, "v2");
    apply(&new, &root, &dir.join("old")).unwrap();
    assert_eq!(read(&root.join("addons").join("mine").join("addon.json")), "{}");
    assert_eq!(read(&root.join("addons").join("other.swoftyaddon")), "user");
    assert!(!root.join("loader").join("stale.dll").exists());
    assert_eq!(read(&root.join("saves").join("p").join("career.sav")), "career");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn without_a_manifest_only_bundled_addon_folders_are_swept() {
    let dir = temp("no-manifest");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    write(&root.join("addons").join("core").join("addon.json"), "v1");
    write(&root.join("addons").join("core").join("removed.txt"), "stale");
    write(&root.join("addons").join("mine").join("addon.json"), "user");
    write(&root.join("addons").join("mine").join("extra.txt"), "user");
    write(&root.join("saves").join("p").join("career.sav"), "career");
    release_tree(&new, "v2");
    write(&new.join("addons").join("core").join("addon.json"), "v2");
    apply(&new, &root, &dir.join("old")).unwrap();
    assert_eq!(read(&root.join("addons").join("core").join("addon.json")), "v2");
    assert!(!root.join("addons").join("core").join("removed.txt").exists());
    assert_eq!(read(&root.join("addons").join("mine").join("addon.json")), "user");
    assert_eq!(read(&root.join("addons").join("mine").join("extra.txt")), "user");
    assert_eq!(read(&root.join("saves").join("p").join("career.sav")), "career");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn only_files_in_the_previous_manifest_are_swept() {
    let dir = temp("manifest-sweep");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    write(&root.join("addons").join("bundled.swoftyaddon"), "old");
    write(&root.join("addons").join("user.swoftyaddon"), "user");
    write(&root.join("loader").join("old.dll"), "old");
    write(&root.join("loader").join("user-added.dll"), "user");
    write(
        &root.join(INSTALL_MANIFEST),
        "addons/bundled.swoftyaddon\nloader/old.dll\n../escape\naddons/../bin/x\n",
    );
    release_tree(&new, "v2");
    apply(&new, &root, &dir.join("old")).unwrap();
    assert!(!root.join("addons").join("bundled.swoftyaddon").exists());
    assert!(!root.join("loader").join("old.dll").exists());
    assert_eq!(read(&root.join("addons").join("user.swoftyaddon")), "user");
    assert_eq!(read(&root.join("loader").join("user-added.dll")), "user");
    let recorded = read(&root.join(INSTALL_MANIFEST));
    assert!(recorded.contains(&format!("loader/{}", crate::addons::builtin::CORE_DLL)));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn replaces_the_install_and_mirrors_the_loader() {
    let dir = temp("apply");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    write(&root.join("loader/stale.dll"), "stale");
    write(&root.join("addons/gone.swoftyaddon"), "gone");
    write(
        &root.join(INSTALL_MANIFEST),
        "loader/stale.dll\naddons/gone.swoftyaddon\naddons/kept.swoftyaddon\ndxc/lib/libold.so\n",
    );
    write(&root.join("addons/kept.swoftyaddon"), "kept v1");
    write(&root.join("dxc/lib/libold.so"), "old dxc");
    write(&root.join("bin/user-note.txt"), "mine");
    release_tree(&new, "v2");
    write(&new.join("dxc/bin").join(exe("dxc")), "dxc v2");
    write(&new.join("share/applications/a.desktop"), "desktop v2");
    write(&new.join("addons/kept.swoftyaddon"), "kept v2");
    apply(&new, &root, &dir.join("old")).unwrap();
    assert_eq!(
        read(&root.join("bin").join(exe("swofty-launcher"))),
        "launcher v2"
    );
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v2");
    assert_eq!(
        read(&root.join("loader").join(crate::addons::builtin::CORE_DLL)),
        "core v2"
    );
    assert_eq!(read(&root.join("share/applications/a.desktop")), "desktop v2");
    assert!(!root.join("loader/stale.dll").exists());
    assert!(!root.join("addons/gone.swoftyaddon").exists());
    assert_eq!(read(&root.join("addons/kept.swoftyaddon")), "kept v2");
    assert_eq!(read(&root.join("dxc/bin").join(exe("dxc"))), "dxc v2");
    assert!(!root.join("dxc/lib/libold.so").exists());
    assert_eq!(read(&root.join("bin/user-note.txt")), "mine");
    assert!(Install::locate(&root.join("bin").join(exe("swofty"))).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_interrupted_update_is_rolled_back_at_the_next_cleanup() {
    let dir = temp("interrupted");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    release_tree(&new, "v2");
    write(&new.join("bin/added"), "v2");
    let work = root.join(STAGING_DIR).join("w1");
    apply(&new, &root, &work.join("old")).unwrap();
    assert!(!work.join(APPLYING).exists());
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v2");
    write(&work.join(APPLYING), "bin/added\n");
    let install = Install { root: root.clone() };
    assert!(install.cleanup());
    assert_eq!(
        read(&root.join("bin").join(exe("swofty-launcher"))),
        "launcher v1"
    );
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v1");
    assert_eq!(
        read(&root.join("loader").join(crate::addons::builtin::CORE_DLL)),
        "core v1"
    );
    assert!(!root.join("bin/added").exists());
    assert!(!root.join(STAGING_DIR).exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cleanup_leaves_a_live_update_alone_and_recovers_a_dead_one() {
    let dir = temp("live");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    release_tree(&new, "v2");
    let work = root.join(STAGING_DIR).join("w1");
    apply(&new, &root, &work.join("old")).unwrap();
    write(&work.join(APPLYING), "");
    write(&work.join("swofty.zip"), "half a download");
    let install = Install { root: root.clone() };

    let live = UpdateLock::try_acquire(&root).expect("no update holds the lock yet");
    assert!(UpdateLock::try_acquire(&root).is_none());
    assert!(!install.cleanup());
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v2");
    assert!(work.join(APPLYING).is_file());
    assert_eq!(read(&work.join("swofty.zip")), "half a download");

    drop(live);
    assert!(install.cleanup());
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v1");
    assert_eq!(
        read(&root.join("loader").join(crate::addons::builtin::CORE_DLL)),
        "core v1"
    );
    assert!(!root.join(STAGING_DIR).exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_failed_replace_leaves_the_old_install() {
    let dir = temp("rollback");
    let (root, new) = (dir.join("install"), dir.join("new"));
    release_tree(&root, "v1");
    write(&root.join("loader/blocked"), "a file where v2 has a folder");
    release_tree(&new, "v2");
    write(&new.join("loader/blocked/inner.dll"), "v2");
    write(&new.join("bin/added"), "v2");
    assert!(apply(&new, &root, &dir.join("old")).is_err());
    assert_eq!(
        read(&root.join("bin").join(exe("swofty-launcher"))),
        "launcher v1"
    );
    assert_eq!(read(&root.join("bin").join(exe("swofty"))), "cli v1");
    assert_eq!(
        read(&root.join("loader").join(crate::addons::builtin::CORE_DLL)),
        "core v1"
    );
    assert_eq!(read(&root.join("loader/blocked")), "a file where v2 has a folder");
    assert!(!root.join("bin/added").exists());
    std::fs::remove_dir_all(&dir).ok();
}
