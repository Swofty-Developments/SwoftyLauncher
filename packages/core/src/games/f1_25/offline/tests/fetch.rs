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
    let failure = download(&url, &mut Vec::new()).unwrap_err();
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
    let still_missing: Vec<String> = REQUIRED[6..].iter().map(|n| url_of(n)).collect();
    assert_eq!(asked, still_missing);
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
    let mut want = vec![url_of(REQUIRED[2])];
    want.extend(REQUIRED[6..].iter().map(|n| url_of(n)));
    assert_eq!(asked, want);
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
    let quiet = ureq::Error::Timeout(ureq::Timeout::PerRead).into_io();
    let stalled = Failure::cut_off(String::new(), Some(&quiet));
    assert!(stalled.transient && stalled.cut_off && !stalled.blocked);
    assert!(!Failure::cut_off(String::new(), Some(&reset)).blocked);
    assert!(!Failure::cut_off(String::new(), None).blocked);
}

fn flaky(
    release: &[(&'static str, String)],
    name: &str,
    cut: usize,
    times: usize,
    range: bool,
    hang: bool,
) -> Reply {
    let body = release
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap()
        .1
        .clone()
        .into_bytes();
    Reply::Flaky {
        body,
        cut,
        times,
        range,
        hang,
    }
}

fn installed(d: &Path, release: &[(&'static str, String)]) {
    for (name, body) in release {
        assert_eq!(
            std::fs::read_to_string(extension_dir(d).join(name)).unwrap(),
            *body
        );
    }
}

#[test]
fn a_cut_off_body_is_resumed_with_a_range_request() {
    let d = temp("resume-range");
    let release = fake_release();
    let name = REQUIRED[2];
    let (base, log) = serve_logged(routes(&release, |n| {
        (n == name).then(|| flaky(&release, n, 10, 2, true, false))
    }));
    fetch_fake(&d, &base, &release).unwrap();
    installed(&d, &release);
    let path = format!("/v9.9.9/{name}");
    let asked: Vec<Option<String>> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, _)| *p == path)
        .map(|(_, r)| r.clone())
        .collect();
    assert_eq!(
        asked,
        vec![None, Some("bytes=10-".to_owned()), Some("bytes=20-".to_owned())]
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_server_that_ignores_range_is_started_over() {
    let d = temp("resume-no-range");
    let release = fake_release();
    let name = REQUIRED[4];
    let (base, log) = serve_logged(routes(&release, |n| {
        (n == name).then(|| flaky(&release, n, 10, 1, false, false))
    }));
    fetch_fake(&d, &base, &release).unwrap();
    installed(&d, &release);
    let path = format!("/v9.9.9/{name}");
    let asked: Vec<Option<String>> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, _)| *p == path)
        .map(|(_, r)| r.clone())
        .collect();
    assert_eq!(asked, vec![None, Some("bytes=10-".to_owned())]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_connection_that_goes_quiet_fails_fast_and_is_resumed() {
    let d = temp("resume-stall");
    let release = fake_release();
    let name = REQUIRED[1];
    let base = serve(routes(&release, |n| {
        (n == name).then(|| flaky(&release, n, 10, 1, true, true))
    }));
    let started = std::time::Instant::now();
    fetch_fake(&d, &base, &release).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    installed(&d, &release);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_download_that_keeps_being_cut_off_says_so_in_plain_words() {
    let d = temp("resume-giveup");
    let marker = previous_install(&d);
    let release = fake_release();
    let name = REQUIRED[3];
    let base = serve(routes(&release, |n| {
        (n == name).then(|| flaky(&release, n, 0, 1000, true, false))
    }));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(
        reason,
        format!("offline play 9.9.9: {name} (after 4 attempts): {CUT_OFF}")
    );
    assert!(CUT_OFF.contains("kept being cut off"));
    assert!(CUT_OFF.contains("VPN or proxy") && CUT_OFF.contains("antivirus") && CUT_OFF.contains("DNS"));
    assert!(!err.contains("blocked on this PC"), "{err}");
    assert!(detail.contains("cut off"), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_resumed_body_that_fails_its_pin_is_never_installed() {
    let d = temp("resume-digest");
    let marker = previous_install(&d);
    let release = fake_release();
    let name = REQUIRED[2];
    let mut wrong = release.clone();
    wrong.iter_mut().find(|(n, _)| *n == name).unwrap().1 = format!("fake {name} but tampered");
    let base = serve(routes(&wrong, |n| {
        (n == name).then(|| flaky(&wrong, n, 10, 1, true, false))
    }));
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    assert!(err.contains("digest mismatch"), "{err}");
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_bad_splice_is_fetched_again_from_the_start() {
    let d = temp("resume-splice");
    let release = fake_release();
    let name = REQUIRED[2];
    let url = url_of(name);
    let whole = body_of(&release, &url);
    let mut calls = 0;
    let mut starts = Vec::new();
    let pins = digests(&release);
    let pins: Vec<(&str, &str)> = pins.iter().map(|(n, h)| (*n, h.as_str())).collect();
    let mut get = |u: &str, body: &mut Vec<u8>| {
        if u != url {
            *body = body_of(&release, u);
            return Ok(());
        }
        starts.push(body.len());
        calls += 1;
        match calls {
            1 => {
                body.extend_from_slice(&whole[..8]);
                Err(reset(u))
            }
            2 => {
                body.extend_from_slice(&vec![b'x'; whole.len() - 8]);
                Ok(())
            }
            _ => {
                *body = whole.clone();
                Ok(())
            }
        }
    };
    fetch_release(&d, FAKE_HOST, "9.9.9", &pins, &mut get, &|_| {}).unwrap();
    installed(&d, &release);
    assert_eq!(starts, vec![0, 8, 0]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_transfer_that_keeps_moving_is_not_given_up_on() {
    let d = temp("resume-progress");
    let release = fake_release();
    let name = REQUIRED[2];
    let url = url_of(name);
    let whole = body_of(&release, &url);
    let pins = digests(&release);
    let pins: Vec<(&str, &str)> = pins.iter().map(|(n, h)| (*n, h.as_str())).collect();
    let mut get = |u: &str, body: &mut Vec<u8>| {
        if u != url {
            *body = body_of(&release, u);
            return Ok(());
        }
        let next = (body.len() + 3).min(whole.len());
        let from = body.len();
        body.extend_from_slice(&whole[from..next]);
        if body.len() < whole.len() {
            Err(reset(u))
        } else {
            Ok(())
        }
    };
    fetch_release(&d, FAKE_HOST, "9.9.9", &pins, &mut get, &|_| {}).unwrap();
    installed(&d, &release);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_server_that_never_sends_headers_is_retried_then_reads_as_cut_off() {
    let d = temp("resume-no-headers");
    let marker = previous_install(&d);
    let release = fake_release();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let accepted = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = accepted.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            held.push(stream);
        }
    });
    let err = fetch_fake(&d, &base, &release).unwrap_err().to_string();
    let (reason, detail) = err.split_once('\n').unwrap();
    assert_eq!(
        reason,
        format!(
            "offline play 9.9.9: {} (after 4 attempts): {CUT_OFF}",
            REQUIRED[0]
        )
    );
    assert!(detail.contains("could not download"), "{err}");
    assert_eq!(accepted.load(std::sync::atomic::Ordering::SeqCst), 4);
    assert_untouched(&d, &marker);
    std::fs::remove_dir_all(&d).ok();
}
