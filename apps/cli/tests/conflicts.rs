use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const ADDONS: [(&str, &str, &str); 2] = [
    (
        "william09leo.heavier-damage-realism",
        "Heavier Damage + 2026 Realism Performance",
        r#","conflicts":[{"id":"formula-nascar.handling"}]"#,
    ),
    ("formula-nascar.handling", "Formula NASCAR", ""),
];

fn data_dir(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-cli-conflicts-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    for (id, name, extra) in ADDONS {
        let dir = root.join("src").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("swofty.json"),
            format!(
                r#"{{"schema_version":1,"id":"{id}","name":"{name}","version":"1.0.0","author":"t","compat":{{"game":"f1_25"}}{extra}}}"#
            ),
        )
        .unwrap();
    }
    root
}

fn swofty(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swofty"))
        .args(args)
        .env("SWOFTY_DATA_DIR", root.join("data"))
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn ok(root: &Path, args: &[&str]) {
    let out = swofty(root, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn two_addons_that_conflict_stop_with_their_names_and_exit_one() {
    let root = data_dir("names");
    for (id, _, _) in ADDONS {
        let src = root.join("src").join(id);
        ok(&root, &["addons", "install", src.to_str().unwrap()]);
        ok(&root, &["addons", "enable", id]);
    }
    let out = swofty(&root, &["registry", "check"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert_eq!(
        stderr.lines().last(),
        Some(
            "swofty: Heavier Damage + 2026 Realism Performance conflicts with Formula NASCAR. Turn one of them \
             off on the Addons page"
        ),
        "{stderr}"
    );
    ok(&root, &["addons", "disable", "formula-nascar.handling"]);
    ok(&root, &["registry", "check"]);
    std::fs::remove_dir_all(&root).ok();
}
