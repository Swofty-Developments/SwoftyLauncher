use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use swofty_core::install::Game;
use swofty_core::online::multiplayer::client::parse_launch_env;
use swofty_core::session::{Config, LaunchRequest, RUNTIME_LIBS_DIR, is_compat_layer};
use swofty_core::store::Store;
use swofty_core::{STEAM_APP_ID, platform::steam};

use crate::console::output::say;

/// Proton's steam helper quote-wraps targets whose path contains spaces
/// and then fails to open the quoted name at all, so the game folder's
/// "F1 25" cannot be passed directly. A space-free symlink to the folder
/// gives an absolute target that survives untouched; the game and the
/// loader see the same files through it.
pub(super) fn game_link(store: &Store, game: &Path) -> Result<PathBuf> {
    let link = store.root.join("state/game-link");
    #[cfg(unix)]
    {
        match std::fs::symlink_metadata(&link) {
            Ok(m) if m.file_type().is_symlink() => {
                if std::fs::read_link(&link).ok().as_deref() != Some(game) {
                    std::fs::remove_file(&link).and_then(|_| std::os::unix::fs::symlink(game, &link))?;
                }
            }
            Ok(_) => anyhow::bail!(
                "{} exists and is not the game link; remove it and try again",
                link.display()
            ),
            Err(_) => {
                std::fs::create_dir_all(store.root.join("state"))?;
                std::os::unix::fs::symlink(game, &link)?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = game;
    Ok(link)
}

/// The game's environment, also listed in the session's env.txt.
pub(super) fn game_env(
    store: &Store,
    config: &Config,
    request: &LaunchRequest,
    g: &Game,
    dir: &Path,
    proton: bool,
    placed_loader: bool,
) -> Result<BTreeMap<String, String>> {
    let winedebug = request
        .winedebug
        .clone()
        .unwrap_or(config.launch.winedebug.clone());
    let mut env = curated_env(config);
    let library_path = if proton {
        steam_runtime_library_path(store, &env)
    } else {
        None
    };
    env.extend(
        [
            ("PROTON_LOG", "1".to_owned()),
            ("PROTON_LOG_DIR", dir.to_string_lossy().into_owned()),
            ("WINEDEBUG", winedebug),
            ("SteamAppId", STEAM_APP_ID.to_string()),
            ("SteamGameId", STEAM_APP_ID.to_string()),
        ]
        .into_iter()
        .chain(library_path.map(|p| ("LD_LIBRARY_PATH", p)))
        .map(|(k, v)| (k.to_owned(), v)),
    );
    if request.steam_saves_once {
        env.insert(swofty_core::saves::launcher::OFF_ENV.to_owned(), "off".to_owned());
    }
    if let Some(armed) = &request.netplay {
        merge_launch_env(&mut env, parse_launch_env(armed));
    }
    if proton && placed_loader {
        name_version_override(&mut env, g);
    }
    let listed: String = env.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(dir.join("env.txt"), listed)?;
    Ok(env)
}

/// The game gets a curated environment: Proton's steam helper packs it
/// into a Windows environment block (32767 characters), and a build
/// shell's variables blow past that, which kills the game at startup.
fn curated_env(config: &Config) -> BTreeMap<String, String> {
    let shell: Vec<(String, String)> = std::env::vars().collect();
    if let Some((k, v)) = shell.iter().find(|(k, _)| is_compat_layer(k))
        && !config.launch.env.keys().any(|k| is_compat_layer(k))
    {
        say(
            "info",
            &format!("environment: not passing the Windows compatibility layer {k}={v} to the game"),
        );
    }
    let shell_vars = shell.iter().filter(|(k, _)| !is_compat_layer(k)).count();
    let env = swofty_core::session::game_env(shell, &config.launch.env);
    if env.len() != shell_vars {
        say(
            "info",
            &format!(
                "environment: passing {} of {} variables to the game (the Windows limit is 32K characters)",
                env.len(),
                shell_vars
            ),
        );
    }
    env
}

/// The library path that lets Proton read wheels and pads through Steam's
/// runtime copy of SDL2 on a system without one.
fn steam_runtime_library_path(store: &Store, env: &BTreeMap<String, String>) -> Option<String> {
    let steam_root = std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH")
        .map(PathBuf::from)
        .or_else(|| steam::find_game().map(|g| g.steam_root))?;
    let path = swofty_core::session::sdl_library_path(
        env.get("LD_LIBRARY_PATH").map(String::as_str),
        &steam_root,
        &store.root.join(RUNTIME_LIBS_DIR),
    )?;
    say(
        "info",
        "controllers: this system has no SDL2, so Proton reads wheels and pads through Steam's runtime copy",
    );
    Some(path)
}

/// An online session's launch variables: a log channel list gains the
/// channels it lacks, and a variable the game already has keeps its value.
fn merge_launch_env(env: &mut BTreeMap<String, String>, launch: Vec<(String, String)>) {
    for (name, value) in launch {
        match env.get_mut(&name) {
            Some(have) if name == "SWOFTY_GAME_LOG_CHANNELS" => {
                let named = |c: &str| c.trim().split(':').next().unwrap_or("").to_owned();
                for channel in value.split(',').map(str::trim) {
                    let name = named(channel);
                    if name.is_empty() || name == "default" || have.split(',').any(|h| named(h) == name) {
                        continue;
                    }
                    if !have.trim().is_empty() {
                        have.push(',');
                    }
                    have.push_str(channel);
                }
            }
            Some(_) => {}
            None => {
                env.insert(name, value);
            }
        }
    }
}

/// The prefix's registry override is not always what Wine reads at start,
/// so the session also names version.dll in WINEDLLOVERRIDES, which wins
/// over the registry.
fn name_version_override(env: &mut BTreeMap<String, String>, g: &Game) {
    let prefix = match g.user_reg.as_deref() {
        Some(reg) if std::fs::read_to_string(reg).is_ok_and(|t| swofty_core::install::has_override(&t)) => {
            format!("{} has the version.dll override", reg.display())
        }
        Some(reg) => format!("{} has no version.dll override", reg.display()),
        None => "no Proton prefix found for this game folder".to_owned(),
    };
    let set = env.get("WINEDLLOVERRIDES").map(String::as_str);
    match swofty_core::session::with_version_override(set) {
        Some(v) => {
            env.insert("WINEDLLOVERRIDES".to_owned(), v);
            say(
                "info",
                &format!("wine: {prefix}; WINEDLLOVERRIDES loads the loader's version.dll"),
            );
        }
        None => say(
            "info",
            &format!(
                "wine: {prefix}; WINEDLLOVERRIDES already names version.dll: {}",
                set.unwrap_or_default()
            ),
        ),
    }
}
