use std::path::Path;

use crate::GAME_EXE;
use crate::diagnostics::quarantine::PreferSource;

use super::{attr, file_name, tidy};

/// Exploit protection's program settings among the Image File Execution
/// Options value names. MaxLoaderThreads alone is common and changes nothing.
pub(super) const EXPLOIT_PROTECTION: &[&str] = &["MitigationOptions", "MitigationAuditOptions", "EAFModules"];

/// Whether a MitigationOptions value turns "prefer System32 DLLs" on: bits
/// 60-61 of its first 64-bit word set to always on.
pub fn prefers_system32(options: &[u8]) -> bool {
    options
        .get(..8)
        .and_then(|b| b.try_into().ok())
        .is_some_and(|b| (u64::from_le_bytes(b) >> 60) & 0x3 == 0x1)
}

/// An Exploit protection XML file's text, which PowerShell writes as UTF-16.
pub fn xml_text(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, rest @ ..] => String::from_utf16_lossy(
            &rest
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        ),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Whether an Exploit protection XML turns PreferSystem32 on for F1_25.exe
/// or for every program.
pub fn policy_prefers(xml: &str) -> bool {
    let lower = xml.to_ascii_lowercase();
    let on = |body: &str| body.contains("prefersystem32=\"true\"") || body.contains("prefersystem32='true'");
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("<appconfig") {
        rest = &rest[at..];
        let end = rest.find("</appconfig>").unwrap_or(rest.len());
        let (block, after) = rest.split_at(end);
        if attr(block, "executable").is_some_and(|e| file_name(e) == GAME_EXE.to_ascii_lowercase())
            && on(block)
        {
            return true;
        }
        rest = after.get(1..).unwrap_or("");
    }
    lower.find("<systemconfig").is_some_and(|at| {
        let block = &lower[at..];
        on(&block[..block.find("</systemconfig>").unwrap_or(block.len())])
    })
}

/// An Exploit protection policy's ExploitProtectionSettings value, which is
/// the XML itself or the file or address that holds it, as a source of
/// "prefer System32 DLLs" when it turns that on or can't be read.
pub fn policy_source(setting: &str) -> Option<PreferSource> {
    let setting = setting.trim();
    if setting.is_empty() {
        return None;
    }
    if setting.starts_with('<') {
        return policy_prefers(setting).then_some(PreferSource::Policy { from: None });
    }
    let local = Path::new(setting).is_absolute() && !setting.starts_with(r"\\");
    let xml = local.then(|| std::fs::read(setting).ok()).flatten();
    let from = Some(tidy(setting));
    match xml {
        Some(bytes) => policy_prefers(&xml_text(&bytes)).then_some(PreferSource::Policy { from }),
        None => Some(PreferSource::Policy { from }),
    }
}
