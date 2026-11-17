use super::*;

#[test]
fn fetch_installs_atomically_over_an_incomplete_copy() {
    let d = temp("fetch-ok");
    let release = fake_release();
    let ext = extension_dir(&d);
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(ext.join(REQUIRED[0]), "half an old install").unwrap();
    std::fs::write(ext.join("stale.bin"), "junk").unwrap();
    std::fs::create_dir_all(d.join("extensions/.download")).unwrap();

    let base = serve(routes(&release, |_| None));
    fetch_fake(&d, &base, &release).unwrap();

    for (name, body) in &release {
        assert_eq!(std::fs::read_to_string(ext.join(name)).unwrap(), *body);
    }
    assert!(
        !ext.join("stale.bin").exists(),
        "the old copy is replaced, not merged"
    );
    assert!(!staging_dir(&d).exists());
    assert!(!d.join("extensions/.offline-play.old").exists());
    assert!(!d.join("extensions/.download").exists());
    #[cfg(unix)]
    for runner in RUNNERS {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(ext.join(runner)).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "{runner} is executable");
    }
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn fetch_names_a_file_missing_from_the_release() {
    let d = temp("fetch-404");
    let marker = previous_install(&d);
    let release = fake_release();
    let missing = RUNNERS[1];
    let base = serve(routes(&release, |n| (n == missing).then_some(Reply::Status(404))));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    assert!(err.contains(missing), "{err}");
    assert!(err.contains("HTTP 404"), "{err}");
    assert!(err.contains("has no such file"), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn fetch_reports_a_server_error() {
    let d = temp("fetch-503");
    let marker = previous_install(&d);
    let release = fake_release();
    let base = serve(routes(&release, |n| {
        (n == REQUIRED[2]).then_some(Reply::Status(503))
    }));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    assert!(err.contains("HTTP 503"), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn fetch_reports_a_cut_off_download() {
    let d = temp("fetch-partial");
    let marker = previous_install(&d);
    let release = fake_release();
    let last = REQUIRED[REQUIRED.len() - 1];
    let base = serve(routes(&release, |n| {
        (n == last).then(|| Reply::CutOff {
            claimed: 4096,
            sent: b"only the start".to_vec(),
        })
    }));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    assert!(err.contains(last), "{err}");
    assert!(err.contains("cut off"), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn fetch_refuses_a_digest_mismatch() {
    let d = temp("fetch-digest");
    let marker = previous_install(&d);
    let release = fake_release();
    let base = serve(routes(&release, |n| {
        (n == REQUIRED[1]).then(|| Reply::Body(b"<html>captive portal</html>".to_vec()))
    }));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    assert!(err.contains("digest mismatch"), "{err}");
    assert!(err.contains(&format!("/v9.9.9/{}", REQUIRED[1])), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn fetch_reports_an_unreachable_host() {
    let d = temp("fetch-refused");
    let marker = previous_install(&d);
    // Port 1 is below every system's ephemeral range, so no other test's server
    // can be listening there; a port freed by a bind could be taken in between.
    let err = fetch_fake(&d, "http://127.0.0.1:1", &fake_release())
        .unwrap_err()
        .to_string();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(reason, format!("offline play 9.9.9: {BLOCKED}"));
    assert!(
        detail.starts_with("(could not download http://127.0.0.1:1/v9.9.9/"),
        "{err}"
    );
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_reply_that_is_not_tls_reads_as_a_blocked_download() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "https://{}/v9.9.9/{}",
        listener.local_addr().unwrap(),
        REQUIRED[0]
    );
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.read_exact(&mut [0; 5]).ok();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .ok();
        stream.read_to_end(&mut Vec::new()).ok();
    });
    let d = temp("not-tls");
    let err = download(&url, &d.join(REQUIRED[0])).unwrap_err();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(reason, BLOCKED);
    assert!(reason.starts_with("the download was blocked on this PC or network"));
    assert!(detail.contains(&url), "{err}");
    assert!(detail.contains("InvalidContentType"), "{err}");
    assert!(!d.join(REQUIRED[0]).exists());
    std::fs::remove_dir_all(&d).ok();
}
