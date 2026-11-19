use super::*;

#[test]
fn consent_lifecycle() {
    let d = temp("consent");
    assert!(!is_accepted(&d));
    record_accept(&d).unwrap();
    assert!(is_accepted(&d));
    withdraw(&d).unwrap();
    assert!(!is_accepted(&d));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn consent_follows_the_disclosure_text() {
    let d = temp("text");
    record_accept(&d).unwrap();
    let mut c = consent(&d).unwrap();
    c.text_sha256 = "old".into();
    write_json(&consent_path(&d), &c).unwrap();
    assert!(!is_accepted(&d), "changed terms must be accepted again");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn install_rejects_wrong_digests() {
    let d = temp("pins");
    let src = d.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for name in REQUIRED {
        std::fs::write(src.join(name), "not the real thing").unwrap();
    }
    let err = install(&d, &src).unwrap_err().to_string();
    assert!(err.contains("digest mismatch"), "{err}");
    assert!(!extension_dir(&d).join("f1op_hook.dll").exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn install_from_a_folder_leaves_no_staging_behind() {
    let d = temp("install-ok");
    let src = d.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let release = fake_release();
    for (name, body) in &release {
        std::fs::write(src.join(name), body).unwrap();
    }
    let pins = digests(&release);
    let pins: Vec<(&str, &str)> = pins.iter().map(|(n, h)| (*n, h.as_str())).collect();
    install_files(&d, &src, &pins).unwrap();
    for (name, body) in &release {
        assert_eq!(
            std::fs::read(extension_dir(&d).join(name)).unwrap(),
            body.as_bytes()
        );
    }
    assert!(!staging_dir(&d).exists());
    std::fs::remove_dir_all(&d).ok();
}
