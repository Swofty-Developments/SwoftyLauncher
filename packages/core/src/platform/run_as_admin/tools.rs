//! Running a Windows console tool such as reg.exe or whoami and reading its output as text.

#[cfg(windows)]
pub(crate) fn windows_tool(program: &str, args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    Some(tool_text(&out.stdout))
}

/// A Windows tool's piped output as text. Console tools such as reg.exe and
/// whoami write it in the OEM code page (850, 857, ...), so a folder named
/// `Yeni klasör` arrives as bytes that are not UTF-8; output that is UTF-8
/// stays as it is.
#[cfg(windows)]
pub(super) fn tool_text(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    const CP_OEMCP: u32 = 1;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MultiByteToWideChar(code_page: u32, flags: u32, s: *const u8, n: i32, w: *mut u16, wn: i32)
        -> i32;
    }
    let Ok(n) = i32::try_from(bytes.len()) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    // SAFETY: bytes is n bytes long, and a null output with size 0 only asks for the length.
    let len = unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), n, std::ptr::null_mut(), 0) };
    if len <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let mut wide = vec![0u16; len as usize];
    // SAFETY: wide holds len u16s, the size the first call returned for the same input.
    let got = unsafe { MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), n, wide.as_mut_ptr(), len) };
    if got <= 0 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    String::from_utf16_lossy(&wide[..got as usize])
}

#[cfg(not(windows))]
pub(crate) fn windows_tool(_: &str, _: &[&str]) -> Option<String> {
    None
}

/// A Windows tool's output, or why there is none: it could not start,
/// failed (its first error line), or did not finish `within` and was ended.
#[cfg(windows)]
pub(crate) fn windows_tool_within(
    program: &str,
    args: &[&str],
    within: std::time::Duration,
) -> std::result::Result<String, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program} did not start ({e})"))?;
    fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut data = Vec::new();
            if let Some(mut p) = pipe {
                p.read_to_end(&mut data).ok();
            }
            data
        })
    }
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let until = std::time::Instant::now() + within;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < until => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Ok(None) => {
                child.kill().ok();
                child.wait().ok();
                return Err(format!("{program} gave no answer within {} s", within.as_secs()));
            }
            Err(e) => return Err(format!("{program}: {e}")),
        }
    };
    let out = tool_text(&out.join().unwrap_or_default());
    if status.success() {
        return Ok(out);
    }
    let err = tool_text(&err.join().unwrap_or_default());
    let first = err
        .lines()
        .chain(out.lines())
        .map(str::trim)
        .find(|l| !l.is_empty());
    Err(match first {
        Some(line) => line.to_owned(),
        None => format!("{program} failed ({status})"),
    })
}
