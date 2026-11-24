mod broken_addons;
mod bundles;
mod career;
mod crash_record;
mod game_version;
mod graphics;
mod hang;
mod hooks;
mod logos;
mod long_log;
mod memory;
mod saves;
mod slow_init;
mod stall;
mod startup;
mod stuck;
mod system;
mod tyres;
mod withdrawn;

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::launch::LOW_DISK;
use crate::manifest::AddonManifest;
use crate::profile::conflicts::Pair;

use super::broken_addons::*;
use super::career::*;
use super::crash_record::*;
use super::evidence::*;
use super::game_version::*;
use super::graphics::*;
use super::stall::*;
use super::system::*;
use super::tyres::*;
use super::*;

const GPU: &str = "\
2026-10-24 02:52:26.633 [33008] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 33008, 463 s after load, at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-10-24 02:52:26.633 [33008] [crash]   writing 0x1
2026-10-24 02:52:26.633 [33008] [crash]   rsi=00000001c3603440 rdi=00000000887a0005 rbp=00000000887a002b rsp=000000000015be50
2026-10-24 02:52:26.633 [33008] [crash]   stack:
2026-10-24 02:52:26.633 [33008] [crash]   #0  F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4
2026-10-24 02:52:26.633 [33008] [crash]   #1  F1_25.exe+0x39b8802 .xtls fn base+0x39b8510+0x2f2
2026-10-24 02:52:27.289 [20344] [crash] crash report swofty\\crash-20261024-025226.txt: the game aborted itself
2026-10-24 02:52:27.289 [33008] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0x39182a4 .xtls Ne_FatalAbort+0x4 reached the top-level filter; passing it to the game's filter
";

fn crash_in(module: &str) -> String {
    format!(
            "2026-10-26 14:01:42.000 [1] [crash] #1 first-chance (the game may still handle it) BREAKPOINT (0x80000003) on thread 1, 21 s after load, at {module}+0x178fd
2026-10-26 14:01:42.000 [1] [crash]   stack:
2026-10-26 14:01:42.000 [1] [crash]   #0  {module}+0x178fd
2026-10-26 14:01:42.000 [1] [crash]   #1  F1_25.exe+0x1234 .xtls fn base+0x1200+0x34
2026-10-26 14:01:42.100 [1] [crash] unhandled BREAKPOINT (0x80000003) at {module}+0x178fd reached the top-level filter; passing it to the game's filter
"
        )
}

fn ev(loader: &str, files: &[&str]) -> Evidence {
    Evidence {
        loader: loader.to_owned(),
        game_files: files.iter().map(|f| (*f).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn a_plain_game_crash_has_no_diagnosis() {
    assert_eq!(
        diagnose(&ev(&crash_in("F1_25.exe"), &["F1_25.exe", "sl.common.dll"])),
        None
    );
    assert_eq!(diagnose(&ev("", &[])), None);
}

fn with_names(mut e: Evidence, names: &[(&str, &str)]) -> Evidence {
    e.names = names
        .iter()
        .map(|(id, n)| ((*id).to_owned(), (*n).to_owned()))
        .collect();
    e.enabled = e.names.keys().cloned().collect();
    e
}

fn perf(rows: &[&str], uptime: u32) -> String {
    format!(
        "Swofty Launcher performance report (format 1)\n\
             == Summary\n\
             uptime: {uptime} s\n\
             \n\
             == Timeline (last 15; each row runs until the next one)\n\
             at          took ms   frames   slow   worst ms  swofty ms  what\n{}\n",
        rows.join("\n")
    )
}

const SPLASH: &[&str] = &[
    "00:00:37      44811     2157      4        624      33.71  screen splash  [core flow tick 20.39 ms]",
    "00:01:27       1772      105      0         18      57.27  flow #a4d3a4b2 Exiting; flow #04fc8ffa Entering  [core flow tick 50.48 ms]",
    "00:01:29     232600    13654      7       1866     331.45  screen splash_transition  [core flow tick 306.18 ms]",
    "00:05:21      86418     5061      3       1219          -  flow #04fc8ffa Exiting (current)",
];

const LIVERY: (&str, &str) = ("crew8006.bp-ultimate-livery", "BP Ultimate Livery");

const RTSS_STRAY_JUMP: &str = "\
[offline] module: RTSSHooks64.dll @ 0x180000000
[offline] module: dxgi.dll @ 0x7fffa3b20000
2026-11-15 07:12:24.289 [ 1212] [d3d12] swapchain 000000006B9A5530 created through CreateSwapChainForHwnd on queue 00000000812DD3C0 (type 0, 1600x1024, format 28, 3 buffer(s)); created by sl.interposer.dll <- F1_25.exe <- KERNEL32.DLL <- ntdll.dll
2026-11-15 07:12:26.735 [ 1212] [crash] #1 first-chance (the game may still handle it) ACCESS_VIOLATION (0xc0000005) on thread 1212, 20 s after load, at 0x7c3615b0 (no module)
2026-11-15 07:12:26.735 [ 1212] [crash]   executing (DEP) 0x7c3615b0
2026-11-15 07:12:26.735 [ 1212] [crash]   rax=0000000080070057 rbx=00000000865546b0 rcx=0000000000000000 rdx=0000000000010000
2026-11-15 07:12:26.735 [ 1212] [crash]   rsi=000000018024056d rdi=00007fff2a76b566 rbp=0000000180073c51 rsp=000000000015c590
2026-11-15 07:12:26.735 [ 1212] [crash]   r8 =7ffffffffffffffc r9 =0000000000000000 r10=00007fffa78fb000 r11=000000000015c320
2026-11-15 07:12:26.735 [ 1212] [crash]   r12=0000000000000003 r13=000000006b9a5530 r14=0000000000000400 r15=0000000000000640
2026-11-15 07:12:26.735 [ 1212] [crash]   code: FF 00 00 00 FF 00 00 00 B4 13 6E 46 FF 17 00 88
2026-11-15 07:12:26.735 [ 1212] [crash]   stack:
2026-11-15 07:12:26.735 [ 1212] [crash]   #0  0x7c3615b0 (no module)
2026-11-15 07:12:26.735 [ 1212] [crash]   #1  0x64b82cb0 (no module)
";
