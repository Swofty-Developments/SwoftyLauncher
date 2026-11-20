use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use swofty_core::diagnostics::send_logs::{self, Bundle, GameInfo, LogServer, PART_LIMIT, Report, Stage};
use swofty_core::store::Store;

#[derive(Args, Debug)]
pub struct SendArgs {
    #[arg(
        long,
        value_name = "USERNAME",
        help = "Your Discord username, so the developers can reply (default: the last one used)"
    )]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "What happened and what you were doing, 10 to 1000 characters (needed unless --dry-run)"
    )]
    pub message: Option<String>,
    #[arg(
        long,
        value_name = "ZIP",
        num_args = 0..=1,
        help = "Write the zip here (a file or a folder; default: the current folder) and list what it holds, without sending"
    )]
    pub dry_run: Option<Option<PathBuf>>,
}

pub fn send(store: &Store, args: SendArgs) -> Result<ExitCode> {
    send_to(store, args, send_logs::destination())
}

pub fn send_to(store: &Store, args: SendArgs, to: Option<LogServer>) -> Result<ExitCode> {
    let remembered = store.config().discord_username;
    let raw = args
        .name
        .or(remembered)
        .context("pass --name <your Discord username> so the developers can reply")?;
    let name = send_logs::check_name(&raw).map_err(|e| anyhow!(e))?;
    let message = match args.message.as_deref() {
        Some(raw) => send_logs::check_message(raw).map_err(|e| anyhow!(e))?,
        None if args.dry_run.is_some() => String::new(),
        None => bail!("pass --message \"<what happened>\" so the developers know what went wrong"),
    };
    if args.dry_run.is_none() && to.is_none() {
        bail!(
            "{}; add --dry-run to write the zip without sending it",
            send_logs::UNAVAILABLE
        );
    }
    if !send_logs::has_session(store) {
        eprintln!(
            "swofty: we don't see a game launch in your logs yet; start the game and reproduce the problem first, then send logs"
        );
    }
    eprintln!("swofty: {}", Stage::Collecting);
    let report = Report {
        name: name.clone(),
        message,
        game: GameInfo::detect(store),
        room: None,
        automatic: None,
    };
    let bundle = Bundle::collect(store, &report);
    if let Some(out) = args.dry_run {
        return dry_run(&bundle, out);
    }
    let kept = bundle.save(&send_logs::kept_dir(store), send_logs::KEPT);
    let sent = to
        .context(send_logs::UNAVAILABLE)?
        .send(&bundle, &|stage| eprintln!("swofty: {stage}"));
    match (&kept, &sent) {
        (Ok(zip), Ok(_)) => println!("{} ({})", zip.display(), send_logs::size(zip_len(zip))),
        (Ok(zip), Err(_)) => eprintln!(
            "swofty: the zip is at {}; you can send it yourself",
            zip.display()
        ),
        (Err(e), _) => eprintln!("swofty: could not keep a copy of the zip: {e}"),
    }
    sent.map_err(|e| anyhow!(e))?;
    let mut config = store.config();
    if config.discord_username.as_deref() != Some(name.as_str()) {
        config.discord_username = Some(name);
        store.save_config(&config)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn zip_len(zip: &Path) -> u64 {
    std::fs::metadata(zip).map_or(0, |m| m.len())
}

fn dry_run(bundle: &Bundle, out: Option<PathBuf>) -> Result<ExitCode> {
    let out = match out {
        Some(p) if p.is_dir() => p.join(bundle.file_name()),
        Some(p) => p,
        None => PathBuf::from(bundle.file_name()),
    };
    let zip = bundle.zip()?;
    std::fs::write(&out, &zip).with_context(|| format!("writing {}", out.display()))?;
    println!("{} ({})", out.display(), send_logs::size(zip.len() as u64));
    for line in bundle.listing() {
        println!("  {line}");
    }
    for s in &bundle.skipped {
        println!("  left out: {s}");
    }
    let parts = bundle.parts(PART_LIMIT)?.len();
    if parts > 1 {
        println!("Sending it would take {parts} messages.");
    }
    if !send_logs::available() {
        println!("{}.", send_logs::UNAVAILABLE);
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn logs_send_dry_run_writes_the_zip() {
        let root = std::env::temp_dir().join(format!("swofty-cli-sendlogs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data = root.join("data");
        std::fs::create_dir_all(data.join("state")).unwrap();
        std::fs::write(data.join("state/launcher-run.log"), "hello\n").unwrap();
        std::fs::write(data.join("config.json"), r#"{"discord_username":"remembered"}"#).unwrap();
        let store = Store::new(&data);
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let args = SendArgs {
            name: None,
            message: Some("  it froze on the grid  ".into()),
            dry_run: Some(Some(out.clone())),
        };
        assert_eq!(send(&store, args).unwrap(), ExitCode::SUCCESS);
        let written: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(written.len(), 1);
        let name = written[0].to_string_lossy().into_owned();
        assert!(
            name.starts_with("swofty-logs-remembered-") && name.ends_with(".zip"),
            "{name}"
        );
        let zip = zip_names(&out.join(&name));
        assert!(zip.contains(&"state/launcher-run.log".to_owned()), "{zip:?}");
        assert!(zip.contains(&"summary.txt".to_owned()));
        let bad = SendArgs {
            name: Some("x".into()),
            message: None,
            dry_run: Some(None),
        };
        assert!(send(&store, bad).is_err());
        let unsaid = SendArgs {
            name: None,
            message: None,
            dry_run: Some(Some(out.join("unsaid.zip"))),
        };
        assert_eq!(send(&store, unsaid).unwrap(), ExitCode::SUCCESS);
        assert!(out.join("unsaid.zip").is_file());
        let sending = |message: Option<&str>| SendArgs {
            name: None,
            message: message.map(Into::into),
            dry_run: None,
        };
        let err = send_to(&store, sending(None), None).unwrap_err();
        assert!(err.to_string().contains("pass --message"), "{err}");
        let err = send_to(&store, sending(Some("  it froze ")), None).unwrap_err();
        assert!(err.to_string().contains("at least 10 characters"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }

    fn stub_log_server(code: u16) -> (String, std::thread::JoinHandle<usize>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/logs", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 16 << 10];
            let end = loop {
                if let Some(head) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let text = String::from_utf8_lossy(&buf[..head]).to_ascii_lowercase();
                    let len: usize = text
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    break head + 4 + len;
                }
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed early");
                buf.extend_from_slice(&chunk[..n]);
            };
            while buf.len() < end {
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed early");
                buf.extend_from_slice(&chunk[..n]);
            }
            write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            )
            .unwrap();
            end
        });
        (url, handle)
    }

    #[test]
    fn logs_send_keeps_the_zip_in_the_data_folder() {
        let root = std::env::temp_dir().join(format!("swofty-cli-keeplogs-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data = root.join("data");
        std::fs::create_dir_all(data.join("state")).unwrap();
        std::fs::write(data.join("state/launcher-run.log"), "hello\n").unwrap();
        let store = Store::new(&data);
        let kept = |store: &Store| -> Vec<PathBuf> {
            let mut zips: Vec<PathBuf> =
                std::fs::read_dir(swofty_core::diagnostics::send_logs::kept_dir(store))
                    .map(|d| d.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default();
            zips.sort();
            zips
        };
        let args = |name: &str| SendArgs {
            name: Some(name.into()),
            message: Some("it froze on the grid".into()),
            dry_run: None,
        };

        let (url, server) = stub_log_server(200);
        let hook = swofty_core::diagnostics::send_logs::LogServer::new(Some(&url));
        assert_eq!(send_to(&store, args("sender"), hook).unwrap(), ExitCode::SUCCESS);
        assert!(server.join().unwrap() > 0);
        let zips = kept(&store);
        assert_eq!(zips.len(), 1, "{zips:?}");
        assert_eq!(zips[0].parent(), Some(data.join("logs").as_path()));
        let name = zips[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("swofty-logs-sender-") && name.ends_with(".zip"),
            "{name}"
        );
        assert!(zip_names(&zips[0]).contains(&"state/launcher-run.log".to_owned()));
        assert_eq!(store.config().discord_username.as_deref(), Some("sender"));

        let (url, server) = stub_log_server(400);
        let hook = swofty_core::diagnostics::send_logs::LogServer::new(Some(&url));
        let err = send_to(&store, args("refused"), hook).unwrap_err();
        assert!(err.to_string().contains("HTTP 400"), "{err}");
        server.join().unwrap();
        let zips = kept(&store);
        assert_eq!(zips.len(), 2, "a refused send keeps its zip too: {zips:?}");
        assert!(zips.iter().any(|z| {
            z.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("swofty-logs-refused-")
        }));

        let err = send_to(&store, args("nowhere"), None).unwrap_err();
        assert!(err.to_string().contains("--dry-run"), "{err}");
        assert_eq!(kept(&store).len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    fn zip_names(path: &Path) -> Vec<String> {
        let data = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&data);
        [
            "summary.txt",
            "config.json",
            "addons.json",
            "state/launcher-run.log",
        ]
        .iter()
        .filter(|n| text.contains(*n))
        .map(|n| (*n).to_owned())
        .collect()
    }
}
