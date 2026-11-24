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
    assert!(!download_dir(&d).exists());
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
    assert!(
        !err.contains("attempts"),
        "a missing file is not asked for again: {err}"
    );
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
    assert!(
        err.contains(&format!("{} (after 4 attempts)", REQUIRED[2])),
        "{err}"
    );
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
    assert!(err.contains("(after 4 attempts)"), "{err}");
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
    assert_eq!(reason, format!("offline play 9.9.9: {}: {BLOCKED}", REQUIRED[0]));
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
    let failure = download(&url).unwrap_err();
    assert!(failure.blocked);
    let err = failure.to_string();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(reason, BLOCKED);
    assert!(reason.starts_with("the download was blocked on this PC or network"));
    assert!(detail.contains(&url), "{err}");
    assert!(detail.contains("InvalidContentType"), "{err}");
}

#[test]
fn a_connection_reset_is_tried_again_until_the_file_arrives() {
    let d = temp("retry-reset");
    let release = fake_release();
    let flaky = url_of(REQUIRED[6]);
    let mut asked = Vec::new();
    let slept = std::cell::RefCell::new(Vec::new());
    let mut get = |url: &str| {
        asked.push(url.to_owned());
        if url == flaky && asked.iter().filter(|u| **u == flaky).count() <= 2 {
            return Err(reset(url));
        }
        Ok(body_of(&release, url))
    };
    fetch_from(&d, &release, &mut get, &|wait| slept.borrow_mut().push(wait)).unwrap();
    for (name, body) in &release {
        assert_eq!(
            std::fs::read_to_string(extension_dir(&d).join(name)).unwrap(),
            *body
        );
    }
    assert_eq!(asked.iter().filter(|u| **u == flaky).count(), 3);
    assert_eq!(asked.len(), REQUIRED.len() + 2);
    assert_eq!(slept.borrow().len(), 2);
    assert!(!download_dir(&d).exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_reset_that_persists_fails_naming_the_file_and_the_attempts() {
    let d = temp("retry-persist");
    let marker = previous_install(&d);
    let release = fake_release();
    let flaky = url_of(REQUIRED[6]);
    let mut asked = Vec::new();
    let mut get = |url: &str| {
        asked.push(url.to_owned());
        if url == flaky {
            Err(reset(url))
        } else {
            Ok(body_of(&release, url))
        }
    };
    let err = fetch_from(&d, &release, &mut get, &|_| {})
        .unwrap_err()
        .to_string();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(
        reason,
        format!(
            "offline play 9.9.9: {} (after 4 attempts): {BLOCKED}",
            REQUIRED[6]
        )
    );
    assert!(
        detail.starts_with(&format!("(could not download {flaky}: ")),
        "{err}"
    );
    assert_eq!(asked.iter().filter(|u| **u == flaky).count(), 4);
    assert!(!asked.contains(&url_of(REQUIRED[7])), "the files after it wait");
    assert_untouched(&d, &marker);
    for name in &REQUIRED[..6] {
        assert!(download_dir(&d).join(name).is_file(), "{name} is kept");
    }
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_pin_mismatch_or_a_missing_file_is_not_tried_again() {
    let release = fake_release();
    for (case, wrong) in [("mismatch", REQUIRED[1]), ("missing", REQUIRED[3])] {
        let d = temp(&format!("retry-{case}"));
        let marker = previous_install(&d);
        let mut asked = Vec::new();
        let mut get = |url: &str| {
            asked.push(url.to_owned());
            match url {
                u if u == url_of(REQUIRED[1]) && case == "mismatch" => {
                    Ok(b"<html>captive portal</html>".to_vec())
                }
                u if u == url_of(REQUIRED[3]) && case == "missing" => Err(Failure::status(u, 404)),
                u => Ok(body_of(&release, u)),
            }
        };
        let err = fetch_from(&d, &release, &mut get, &|_| panic!("no wait for {case}"))
            .unwrap_err()
            .to_string();
        let wanted = if case == "mismatch" {
            "digest mismatch"
        } else {
            "has no such file"
        };
        assert!(err.contains(wanted), "{err}");
        assert!(!err.contains("attempts"), "{err}");
        assert_eq!(asked.iter().filter(|u| **u == url_of(wrong)).count(), 1);
        assert!(!download_dir(&d).join(wrong).exists(), "{case}: nothing is kept");
        assert_untouched(&d, &marker);
        std::fs::remove_dir_all(&d).ok();
    }
}

#[test]
fn the_next_fetch_asks_only_for_the_files_still_missing() {
    let d = temp("retry-resume");
    let release = fake_release();
    let broken = url_of(REQUIRED[6]);
    let mut first = |url: &str| {
        if url == broken {
            Err(reset(url))
        } else {
            Ok(body_of(&release, url))
        }
    };
    fetch_from(&d, &release, &mut first, &|_| {}).unwrap_err();

    let mut asked = Vec::new();
    let mut second = |url: &str| {
        asked.push(url.to_owned());
        Ok(body_of(&release, url))
    };
    fetch_from(&d, &release, &mut second, &|_| {}).unwrap();
    assert_eq!(asked, [url_of(REQUIRED[6]), url_of(REQUIRED[7])]);
    for (name, body) in &release {
        assert_eq!(
            std::fs::read_to_string(extension_dir(&d).join(name)).unwrap(),
            *body
        );
    }
    assert!(!download_dir(&d).exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_kept_file_that_no_longer_matches_its_pin_is_fetched_again() {
    let d = temp("retry-stale");
    let release = fake_release();
    let broken = url_of(REQUIRED[6]);
    let mut first = |url: &str| {
        if url == broken {
            Err(reset(url))
        } else {
            Ok(body_of(&release, url))
        }
    };
    fetch_from(&d, &release, &mut first, &|_| {}).unwrap_err();
    std::fs::write(download_dir(&d).join(REQUIRED[2]), "changed after the download").unwrap();

    let mut asked = Vec::new();
    let mut second = |url: &str| {
        asked.push(url.to_owned());
        Ok(body_of(&release, url))
    };
    fetch_from(&d, &release, &mut second, &|_| {}).unwrap();
    assert_eq!(
        asked,
        [url_of(REQUIRED[2]), url_of(REQUIRED[6]), url_of(REQUIRED[7])]
    );
    assert_eq!(
        std::fs::read_to_string(extension_dir(&d).join(REQUIRED[2])).unwrap(),
        release[2].1
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn only_failures_a_new_try_can_get_past_are_tried_again() {
    use std::io::ErrorKind;
    let url = url_of(REQUIRED[0]);
    for kind in [
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::TimedOut,
        ErrorKind::UnexpectedEof,
    ] {
        let f = Failure::call(&url, &ureq::Error::Io(kind.into()));
        assert!(f.transient && f.blocked, "{kind:?}");
    }
    let dns =
        std::io::Error::other("failed to lookup address information: Temporary failure in name resolution");
    assert!(Failure::call(&url, &ureq::Error::Io(dns)).transient);
    assert!(Failure::call(&url, &ureq::Error::Timeout(ureq::Timeout::Connect)).transient);
    assert!(Failure::call(&url, &ureq::Error::ConnectionFailed).transient);
    assert!(!Failure::call(&url, &ureq::Error::Timeout(ureq::Timeout::Global)).transient);
    let refused = Failure::call(&url, &ureq::Error::Io(ErrorKind::ConnectionRefused.into()));
    assert!(refused.blocked && !refused.transient);
    let untrusted = rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer);
    let f = Failure::call(&url, &ureq::Error::Rustls(untrusted));
    assert!(f.blocked && !f.transient);
    for status in [429, 500, 502, 503, 504] {
        assert!(Failure::status(&url, status).transient, "{status}");
    }
    for status in [400, 403, 404, 410] {
        assert!(!Failure::status(&url, status).transient, "{status}");
    }
    let reset = std::io::Error::from(ErrorKind::ConnectionReset);
    assert!(Failure::cut_off(String::new(), Some(&reset)).transient);
    assert!(Failure::cut_off(String::new(), None).transient);
    let late = ureq::Error::Timeout(ureq::Timeout::Global).into_io();
    let stalled = Failure::cut_off(String::new(), Some(&late));
    assert!(!stalled.transient && stalled.blocked);
    assert!(!Failure::cut_off(String::new(), Some(&reset)).blocked);
    assert!(!Failure::cut_off(String::new(), None).blocked);
}
