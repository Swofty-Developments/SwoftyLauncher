use super::*;

#[cfg(unix)]
#[test]
fn a_supported_build_is_probed_once_until_the_exe_changes() {
    use std::os::unix::fs::PermissionsExt;
    let d = temp("probe-cache");
    let (data, game) = (d.join("data"), d.join("game"));
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join(crate::GAME_EXE), "exe").unwrap();
    let runs = d.join("runs");
    let probe = d.join("probe.sh");
    std::fs::write(
        &probe,
        format!(
            "#!/bin/sh\necho run >> '{}'\necho supported build\n",
            runs.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let a = Artifacts {
        dir: d.clone(),
        hook: d.join("hook"),
        stub: d.join("stub"),
        prelaunch: d.join("prelaunch"),
        probe,
    };
    let count = || std::fs::read_to_string(&runs).unwrap_or_default().lines().count();
    for _ in 0..2 {
        let p = probe_cached(&data, &game, &a).unwrap();
        assert!(p.supported);
        assert_eq!(p.report, "supported build");
    }
    assert_eq!(count(), 1);
    std::fs::write(game.join(crate::GAME_EXE), "updated exe").unwrap();
    assert!(probe_cached(&data, &game, &a).unwrap().supported);
    assert_eq!(count(), 2);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_missing_runner_is_never_started() {
    let d = temp("no-runner");
    let a = Artifacts {
        hook: d.join("f1op_hook.dll"),
        stub: d.join("f1op_stub.exe"),
        prelaunch: d.join(RUNNERS[0]),
        probe: d.join(RUNNERS[1]),
        dir: d.clone(),
    };
    let err = probe(&d, &a).unwrap_err().to_string();
    assert!(err.contains("is missing"), "{err}");
    assert!(err.contains("swofty offline fetch"), "{err}");
    let err = stage(&d.join("game"), &a, &d.join("state/offline/staging.jsonl"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("is missing"), "{err}");
    std::fs::remove_dir_all(&d).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn a_runner_whose_loader_is_missing_runs_through_the_system_loader() {
    let d = temp("loader");
    let Some((runner, real)) = binary_with_a_missing_loader(&d) else {
        eprintln!("skipping: the test binary is statically linked");
        return;
    };
    let direct = Command::new(&runner).arg("--list").output().unwrap_err();
    assert_eq!(
        direct.kind(),
        std::io::ErrorKind::NotFound,
        "the Steam Deck symptom"
    );

    let dirs = [real.parent().unwrap()];
    let exec = exec_mode_in(&runner, &dirs).unwrap();
    assert_eq!(
        exec,
        Exec::Loader {
            wanted: elf_interpreter(&runner).unwrap().unwrap(),
            system: real.clone(),
        }
    );
    let out = command_for(&runner, exec).arg("--list").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    std::fs::remove_dir_all(&d).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn a_runner_with_no_usable_loader_is_explained() {
    let d = temp("no-loader");
    let Some((runner, _)) = binary_with_a_missing_loader(&d) else {
        eprintln!("skipping: the test binary is statically linked");
        return;
    };
    let err = exec_mode_in(&runner, &[]).unwrap_err().to_string();
    let wanted = elf_interpreter(&runner).unwrap().unwrap();
    assert!(err.contains("cannot run on this system"), "{err}");
    assert!(err.contains(&wanted.display().to_string()), "{err}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_runner_that_is_not_elf_runs_directly() {
    let d = temp("not-elf");
    let runner = d.join("runner");
    std::fs::write(&runner, "#!/bin/sh\n").unwrap();
    assert_eq!(exec_mode_in(&runner, &[]).unwrap(), Exec::Direct);
    std::fs::remove_dir_all(&d).ok();
}
