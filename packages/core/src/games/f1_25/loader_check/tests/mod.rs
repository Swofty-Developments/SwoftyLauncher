use std::path::PathBuf;

use super::paths::{tidy_with, zone_id};
use super::*;
use crate::diagnostics::quarantine::{Carrier, Cause};
use crate::game_folder::baseline::sha256_file;

mod attach;
mod cause;
mod lines;
mod reading;

const GAME: &str = r"E:\SteamLibrary\steamapps\common\F1 25";
const SYSTEM: &str = r"C:\WINDOWS\SYSTEM32\VERSION.dll";

fn ci(id: u32, file: &str, policy: Option<&str>) -> String {
    format!(
        "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
         <Provider Name='Microsoft-Windows-CodeIntegrity'/><EventID>{id}</EventID><Level>2</Level>\
         <TimeCreated SystemTime='2026-10-23T16:27:09.6291234Z'/><Computer>MATTHIJS</Computer>\
         <Security UserID='S-1-5-21-1-2-3-1001'/></System><EventData>\
         <Data Name='File Name Length'>63</Data><Data Name='File Name'>{file}</Data>\
         <Data Name='Process Name'>\\Device\\HarddiskVolume3\\SteamLibrary\\steamapps\\common\\F1 25\\F1_25.exe</Data>\
         <Data Name='Requested Signing Level'>8</Data><Data Name='PolicyID'/>{}</EventData></Event>",
        policy
            .map(|p| format!("<Data Name='PolicyName'>{p}</Data>"))
            .unwrap_or_default()
    )
}

const PROXY_PATH: &str = r"\Device\HarddiskVolume3\SteamLibrary\steamapps\common\F1 25\version.dll";

fn game() -> PathBuf {
    PathBuf::from(GAME)
}

fn running(version: &[&str], mitigation: Mitigation) -> Seen {
    Seen {
        pid: 27488,
        image: Some(format!(r"{GAME}\F1_25.exe")),
        image_size: Some(423_518_208),
        ours: true,
        modules: Ok(Modules {
            count: 212,
            version: version.iter().map(|v| (*v).to_owned()).collect(),
            others: Vec::new(),
            shims: Vec::new(),
        }),
        mitigation: Some(Ok(mitigation)),
    }
}

fn found(events: Vec<Event>, games: Vec<Seen>) -> Findings {
    Findings {
        games,
        logs: vec![EventLog {
            name: "Code Integrity",
            read: Ok(events),
        }],
        search: Some(Ok(Search::default())),
        smart_app_control: Some(Ok(Some(0))),
        ..Findings::default()
    }
}

fn system_copy() -> Findings {
    found(Vec::new(), vec![running(&[SYSTEM], Mitigation::default())])
}

fn with_modules(f: &mut Findings, others: &[&str], shims: &[&str]) {
    let Ok(m) = &mut f.games[0].modules else {
        panic!("modules read")
    };
    m.others = others.iter().map(|o| (*o).to_owned()).collect();
    m.shims = shims.iter().map(|o| (*o).to_owned()).collect();
}

fn starter(name: &str, exe: &str, layers: &str, all_users: bool) -> AdminLayer {
    AdminLayer {
        all_users,
        name: name.into(),
        exe: exe.into(),
        layers: layers.into(),
    }
}

const WAITING: &str = "2026-11-13 19:02:11.120 [ 8124] swofty loader (version.dll proxy) attached, pid 12320\r\n\
     2026-11-13 19:02:11.121 [ 8124] host: C:\\Games\\F1 25\\F1_25.exe\r\n\
     2026-11-13 19:02:11.180 [ 8124] trigger armed: game IAT slot USER32.dll!RegisterClassExW at 0x1 (was 0x2)\r\n\
     2026-11-13 19:02:11.180 [ 8124] trigger armed: game IAT slot USER32.dll!CreateWindowExW at 0x3 (was 0x4)\r\n\
     2026-11-13 19:02:11.181 [ 8124] trigger armed: game IAT slot KERNEL32.dll!CreateMutexW at 0x5 (was 0x6)\r\n\
     2026-11-13 19:02:11.420 [ 9940] fallback watcher: waiting for a visible main window if no trigger fires \
     (timeout 300 s)\r\n";

fn waiting() -> Findings {
    let mut f = found(
        Vec::new(),
        vec![running(&[&format!(r"{GAME}\version.dll")], Mitigation::default())],
    );
    f.games[0].pid = 12320;
    f.rotated = true;
    f.attached = attached(WAITING, &[12320], true).map(|a| Attached {
        ran: Some(Duration::from_secs(46)),
        ..a
    });
    f
}
