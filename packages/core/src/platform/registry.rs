//! The Windows registry read in this process, written out as `reg query` prints it, so the parsers made
//! for `reg query` output read it unchanged. Starting reg.exe costs about half a second a call on a busy
//! computer, and looking for the game's copies queries several keys.

/// A registry value's data, by its type.
#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Sz(String),
    ExpandSz(String),
    MultiSz(Vec<String>),
    Dword(u32),
    Qword(u64),
    Binary(Vec<u8>),
    /// A type `reg query` prints as bytes: REG_NONE and the rarer ones.
    Other(&'static str, Vec<u8>),
}

/// One key as `reg query` lists it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Key {
    /// The full name, the hive spelt out: `HKEY_LOCAL_MACHINE\SOFTWARE\...`.
    pub path: String,
    /// Each value in the order the registry keeps them; the unnamed default value is named "".
    pub values: Vec<(String, Data)>,
    /// The subkeys' names, without `/s`; with it, the subkeys themselves.
    pub subkeys: Vec<String>,
    pub children: Vec<Key>,
}

/// What `reg query` is asked: `/s` lists every subkey's values below the key, `/ve` only the unnamed
/// default value, `/reg:32` the 32-bit view of a 64-bit Windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ask {
    pub all: bool,
    pub default_only: bool,
    pub view32: bool,
}

/// The key and the switches of `reg query` arguments (`query` itself left out), or None for a switch
/// the launcher never passes.
pub fn parse_args<'a>(args: &[&'a str]) -> Option<(&'a str, Ask)> {
    let (key, switches) = args.split_first()?;
    let mut ask = Ask::default();
    for s in switches {
        match s.to_ascii_lowercase().as_str() {
            "/s" => ask.all = true,
            "/ve" => ask.default_only = true,
            "/reg:64" => ask.view32 = false,
            "/reg:32" => ask.view32 = true,
            _ => return None,
        }
    }
    Some((key, ask))
}

/// The hive a key starts with, its full name and its predefined handle value.
pub fn hive(key: &str) -> Option<(&'static str, u32, &str)> {
    let (head, rest) = key.split_once('\\').unwrap_or((key, ""));
    let (name, handle) = match head.to_ascii_uppercase().as_str() {
        "HKCR" | "HKEY_CLASSES_ROOT" => ("HKEY_CLASSES_ROOT", 0x8000_0000),
        "HKCU" | "HKEY_CURRENT_USER" => ("HKEY_CURRENT_USER", 0x8000_0001),
        "HKLM" | "HKEY_LOCAL_MACHINE" => ("HKEY_LOCAL_MACHINE", 0x8000_0002),
        "HKU" | "HKEY_USERS" => ("HKEY_USERS", 0x8000_0003),
        "HKCC" | "HKEY_CURRENT_CONFIG" => ("HKEY_CURRENT_CONFIG", 0x8000_0005),
        _ => return None,
    };
    Some((name, handle, rest.trim_end_matches('\\')))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn value_line(out: &mut String, name: &str, data: &Data) {
    let name = if name.is_empty() { "(Default)" } else { name };
    let (kind, text) = match data {
        Data::Sz(s) => ("REG_SZ", s.clone()),
        Data::ExpandSz(s) => ("REG_EXPAND_SZ", s.clone()),
        Data::MultiSz(items) => ("REG_MULTI_SZ", items.join("\\0")),
        Data::Dword(n) => ("REG_DWORD", format!("0x{n:x}")),
        Data::Qword(n) => ("REG_QWORD", format!("0x{n:x}")),
        Data::Binary(b) => ("REG_BINARY", hex(b)),
        Data::Other(kind, b) => (*kind, hex(b)),
    };
    out.push_str(&format!("    {name}    {kind}    {text}\r\n"));
}

/// `key` as `reg query` prints it with `ask`.
pub fn render(key: &Key, ask: Ask) -> String {
    let mut out = String::from("\r\n");
    render_into(&mut out, key, ask);
    out
}

fn render_into(out: &mut String, key: &Key, ask: Ask) {
    out.push_str(&key.path);
    out.push_str("\r\n");
    if ask.default_only {
        match key.values.iter().find(|(name, _)| name.is_empty()) {
            Some((name, data)) => value_line(out, name, data),
            None => out.push_str("    (Default)    REG_SZ    (value not set)\r\n"),
        }
        out.push_str("\r\n");
        return;
    }
    for (name, data) in &key.values {
        value_line(out, name, data);
    }
    out.push_str("\r\n");
    if ask.all {
        for child in &key.children {
            render_into(out, child, ask);
        }
    } else {
        for sub in &key.subkeys {
            out.push_str(&format!("{}\\{sub}\r\n", key.path));
        }
    }
}

/// The data a value of type `kind` holds in `raw`, as `reg query` reads it.
pub fn data(kind: u32, raw: &[u8]) -> Data {
    let text = |raw: &[u8]| -> String {
        let wide: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        String::from_utf16_lossy(&wide)
    };
    match kind {
        1 => Data::Sz(text(raw)),
        2 => Data::ExpandSz(text(raw)),
        3 => Data::Binary(raw.to_vec()),
        4 if raw.len() >= 4 => Data::Dword(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
        7 => {
            let wide: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let mut items: Vec<String> = wide.split(|&c| c == 0).map(String::from_utf16_lossy).collect();
            while items.last().is_some_and(String::is_empty) {
                items.pop();
            }
            Data::MultiSz(items)
        }
        11 if raw.len() >= 8 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(&raw[..8]);
            Data::Qword(u64::from_le_bytes(b))
        }
        0 => Data::Other("REG_NONE", raw.to_vec()),
        _ => Data::Other("REG_UNKNOWN", raw.to_vec()),
    }
}

/// What `reg query <args>` prints, read in this process; None when the key cannot be opened, where
/// reg.exe printed nothing on its output.
pub fn reg_query(args: &[&str]) -> Option<String> {
    let (key, ask) = parse_args(args)?;
    read(key, ask).map(|k| render(&k, ask))
}

#[cfg(windows)]
pub fn read(key: &str, ask: Ask) -> Option<Key> {
    let (name, handle, rest) = hive(key)?;
    let root = win32::predefined(handle);
    let path = match rest.is_empty() {
        true => name.to_owned(),
        false => format!("{name}\\{rest}"),
    };
    let open = win32::Open::at(root, rest, ask.view32)?;
    Some(win32::key(&open, path, ask))
}

#[cfg(not(windows))]
pub fn read(_: &str, _: Ask) -> Option<Key> {
    None
}

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    use super::{Ask, Key};

    const KEY_READ: u32 = 0x2_0019;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const ERROR_SUCCESS: i32 = 0;
    const ERROR_MORE_DATA: i32 = 234;
    const ERROR_NO_MORE_ITEMS: i32 = 259;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: *mut c_void,
            sub: *const u16,
            options: u32,
            sam: u32,
            out: *mut *mut c_void,
        ) -> i32;
        fn RegEnumKeyExW(
            key: *mut c_void,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            class: *mut u16,
            class_len: *mut u32,
            written: *mut u64,
        ) -> i32;
        fn RegEnumValueW(
            key: *mut c_void,
            index: u32,
            name: *mut u16,
            name_len: *mut u32,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegCloseKey(key: *mut c_void) -> i32;
    }

    pub fn predefined(handle: u32) -> *mut c_void {
        handle as i32 as isize as *mut c_void
    }

    /// An open key, closed when dropped.
    pub struct Open {
        handle: *mut c_void,
        view32: bool,
    }

    impl Open {
        pub fn at(parent: *mut c_void, sub: &str, view32: bool) -> Option<Self> {
            let wide: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
            let view = if view32 { KEY_WOW64_32KEY } else { KEY_WOW64_64KEY };
            let mut handle = std::ptr::null_mut();
            // SAFETY: `wide` is NUL-terminated and outlives the call; `handle` is written on success only.
            let status = unsafe { RegOpenKeyExW(parent, wide.as_ptr(), 0, KEY_READ | view, &mut handle) };
            (status == ERROR_SUCCESS).then_some(Self { handle, view32 })
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: the handle came from RegOpenKeyExW and is closed once.
            unsafe { RegCloseKey(self.handle) };
        }
    }

    fn subkeys(key: &Open) -> Vec<String> {
        let mut out = Vec::new();
        let mut name = vec![0u16; 256];
        for index in 0.. {
            let mut len = name.len() as u32;
            // SAFETY: `name` holds `len` u16s; the optional outputs are null.
            let status = unsafe {
                RegEnumKeyExW(
                    key.handle,
                    index,
                    name.as_mut_ptr(),
                    &mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            match status {
                ERROR_SUCCESS => out.push(String::from_utf16_lossy(&name[..len as usize])),
                _ => break,
            }
        }
        out
    }

    fn values(key: &Open) -> Vec<(String, super::Data)> {
        let mut out = Vec::new();
        let (mut name, mut data) = (vec![0u16; 16_384], vec![0u8; 4096]);
        let mut index = 0;
        loop {
            let (mut name_len, mut data_len, mut kind) = (name.len() as u32, data.len() as u32, 0u32);
            // SAFETY: `name` holds `name_len` u16s and `data` holds `data_len` bytes.
            let status = unsafe {
                RegEnumValueW(
                    key.handle,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    std::ptr::null_mut(),
                    &mut kind,
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            match status {
                ERROR_SUCCESS => {
                    out.push((
                        String::from_utf16_lossy(&name[..name_len as usize]),
                        super::data(kind, &data[..data_len as usize]),
                    ));
                    index += 1;
                }
                ERROR_MORE_DATA if (data_len as usize) > data.len() => data.resize(data_len as usize, 0),
                ERROR_MORE_DATA => data.resize(data.len() * 2, 0),
                ERROR_NO_MORE_ITEMS => break,
                _ => break,
            }
        }
        out
    }

    pub fn key(open: &Open, path: String, ask: Ask) -> Key {
        let values = values(open);
        if ask.default_only {
            return Key {
                path,
                values,
                ..Key::default()
            };
        }
        let subkeys = subkeys(open);
        let children = match ask.all {
            true => subkeys
                .iter()
                .filter_map(|sub| {
                    let child = Open::at(open.handle, sub, open.view32)?;
                    Some(key(&child, format!("{path}\\{sub}"), ask))
                })
                .collect(),
            false => Vec::new(),
        };
        Key {
            path,
            values,
            subkeys,
            children,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::run_as_admin::{parse_reg_query, reg_dword};

    fn sz(s: &str) -> Data {
        Data::Sz(s.into())
    }

    #[test]
    fn a_key_prints_as_reg_query_prints_it() {
        let key = Key {
            path: r"HKEY_LOCAL_MACHINE\SOFTWARE\Codemasters\F1_25".into(),
            values: vec![
                ("Install Dir".into(), sz(r"D:\EA Games\F1 25\")),
                ("Locale".into(), sz("en_US")),
            ],
            ..Key::default()
        };
        assert_eq!(
            render(&key, Ask::default()),
            "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Codemasters\\F1_25\r\n    Install Dir    REG_SZ    D:\\EA Games\\F1 25\\\r\n    Locale    REG_SZ    en_US\r\n\r\n"
        );
    }

    #[test]
    fn each_type_reads_back_through_the_reg_query_parsers() {
        let key = Key {
            path: r"HKEY_CURRENT_USER\Software\Valve\Steam".into(),
            values: vec![
                ("SteamPath".into(), sz("c:/program files (x86)/steam")),
                ("Empty".into(), sz("")),
                (
                    "SteamExe".into(),
                    Data::ExpandSz(r"%ProgramFiles%\Steam\steam.exe".into()),
                ),
                ("pid".into(), Data::Dword(0x1a2b)),
                ("ActiveUser".into(), Data::Dword(0)),
                ("Big".into(), Data::Qword(0x1_0000_0000)),
                ("Blob".into(), Data::Binary(vec![0x0a, 0xff])),
                ("List".into(), Data::MultiSz(vec!["a".into(), "b".into()])),
            ],
            subkeys: vec!["Apps".into()],
            ..Key::default()
        };
        let out = render(&key, Ask::default());
        assert_eq!(
            parse_reg_query(&out),
            [
                ("SteamPath".to_owned(), "c:/program files (x86)/steam".to_owned()),
                ("Empty".to_owned(), String::new()),
                (
                    "SteamExe".to_owned(),
                    r"%ProgramFiles%\Steam\steam.exe".to_owned()
                ),
            ]
        );
        assert_eq!(reg_dword(&out, "pid"), Some(0x1a2b));
        assert_eq!(reg_dword(&out, "activeuser"), Some(0));
        assert!(out.contains("    Big    REG_QWORD    0x100000000\r\n"));
        assert!(out.contains("    Blob    REG_BINARY    0AFF\r\n"));
        assert!(out.contains("    List    REG_MULTI_SZ    a\\0b\r\n"));
        assert!(out.ends_with("\r\n\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\\Apps\r\n"));
    }

    #[test]
    fn the_default_value_alone_and_a_tree_print_as_reg_query_prints_them() {
        let command = r#""C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe" "%1""#;
        let mut key = Key {
            path: r"HKEY_CLASSES_ROOT\origin2\shell\open\command".into(),
            values: vec![("".into(), sz(command)), ("Other".into(), sz("x"))],
            ..Key::default()
        };
        let ve = Ask {
            default_only: true,
            ..Ask::default()
        };
        assert_eq!(
            render(&key, ve),
            format!(
                "\r\nHKEY_CLASSES_ROOT\\origin2\\shell\\open\\command\r\n    (Default)    REG_SZ    {command}\r\n\r\n"
            )
        );
        key.values.remove(0);
        assert!(render(&key, ve).contains("    (Default)    REG_SZ    (value not set)\r\n"));

        let uninstall = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
        let child = |id: &str, name: &str, at: &str| Key {
            path: format!("{uninstall}\\{id}"),
            values: vec![
                ("DisplayName".into(), sz(name)),
                ("InstallLocation".into(), sz(at)),
            ],
            ..Key::default()
        };
        let tree = Key {
            path: uninstall.into(),
            subkeys: vec!["{A}".into(), "{B}".into()],
            children: vec![
                child("{A}", "Other", r"C:\Other"),
                child("{B}", "F1® 25", r#""E:\Games\F1 25""#),
            ],
            ..Key::default()
        };
        let all = Ask {
            all: true,
            ..Ask::default()
        };
        let blocks = crate::platform::ea_app::reg_tree(&render(&tree, all));
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[2].0, format!("{uninstall}\\{{B}}"));
        assert_eq!(
            crate::platform::ea_app::install_dirs(&blocks[2].1),
            [std::path::PathBuf::from(r"E:\Games\F1 25")]
        );
    }

    #[test]
    fn raw_values_read_as_reg_query_reads_them() {
        let wide = |s: &str| -> Vec<u8> { s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect() };
        assert_eq!(data(1, &wide("F1® 25")), sz("F1® 25"));
        assert_eq!(data(2, &wide(r"%A%\b")), Data::ExpandSz(r"%A%\b".into()));
        assert_eq!(data(4, &0x2au32.to_le_bytes()), Data::Dword(42));
        assert_eq!(data(11, &7u64.to_le_bytes()), Data::Qword(7));
        let mut multi = wide("a");
        multi.extend(wide("b"));
        multi.extend([0, 0]);
        assert_eq!(data(7, &multi), Data::MultiSz(vec!["a".into(), "b".into()]));
        assert_eq!(data(1, &[]), sz(""));
    }

    #[test]
    fn the_switches_the_launcher_passes_parse_and_others_do_not() {
        let all = parse_args(&[r"HKLM\X", "/s"]).unwrap();
        assert_eq!(
            all,
            (
                r"HKLM\X",
                Ask {
                    all: true,
                    ..Ask::default()
                }
            )
        );
        assert!(parse_args(&[r"HKCR\o", "/ve"]).unwrap().1.default_only);
        assert!(!parse_args(&[r"HKCU\L", "/reg:64"]).unwrap().1.view32);
        assert!(parse_args(&[r"HKLM\X", "/f", "x"]).is_none());
        assert_eq!(
            hive(r"hkcu\Software\Valve\"),
            Some(("HKEY_CURRENT_USER", 0x8000_0001, r"Software\Valve"))
        );
        assert_eq!(
            hive(r"HKEY_LOCAL_MACHINE\SOFTWARE"),
            Some(("HKEY_LOCAL_MACHINE", 0x8000_0002, "SOFTWARE"))
        );
        assert_eq!(hive(r"HKXX\a"), None);
    }

    /// The in-process read and reg.exe agree on keys every Windows has, value by value as the parsers
    /// see them. Values with characters outside ASCII are left out: reg.exe writes them in the console's
    /// code page, which may not hold them.
    #[cfg(windows)]
    #[test]
    fn the_in_process_read_agrees_with_reg_exe() {
        use crate::platform::ea_app::reg_tree;
        let ascii = |pairs: Vec<(String, String)>| -> Vec<(String, String)> {
            pairs
                .into_iter()
                .filter(|(n, d)| n.is_ascii() && d.is_ascii())
                .collect()
        };
        for args in [
            &[r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion"][..],
            &[r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/reg:64"][..],
            &[r"HKCU\Environment"][..],
            &[r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", "/s"][..],
            &[
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
                "/s",
            ][..],
        ] {
            let mut all = vec!["query"];
            all.extend_from_slice(args);
            let start = std::time::Instant::now();
            let tool = crate::platform::run_as_admin::windows_tool("reg", &all).unwrap_or_default();
            let tool_took = start.elapsed();
            let start = std::time::Instant::now();
            let here = reg_query(args).unwrap_or_default();
            let here_took = start.elapsed();
            eprintln!("reg query {args:?}: reg.exe {tool_took:?}, in process {here_took:?}");
            let blocks = |out: &str| -> Vec<(String, Vec<(String, String)>)> {
                reg_tree(out)
                    .into_iter()
                    .filter(|(h, _)| h.is_ascii())
                    .map(|(h, v)| (h.to_ascii_lowercase(), ascii(parse_reg_query(&v))))
                    .filter(|(_, v)| !v.is_empty())
                    .collect()
            };
            assert!(!blocks(&here).is_empty(), "{args:?}");
            assert_eq!(blocks(&here), blocks(&tool), "{args:?}");
            for line in tool.lines().filter(|l| l.contains("    REG_DWORD    ")) {
                let name = line.trim_start().split("    ").next().unwrap_or_default();
                assert_eq!(reg_dword(&here, name), reg_dword(&tool, name), "{args:?} {name}");
            }
        }
        let ve = [r"HKCR\.txt", "/ve"];
        assert_eq!(
            parse_reg_query(&reg_query(&ve).unwrap()),
            parse_reg_query(
                &crate::platform::run_as_admin::windows_tool("reg", &["query", ve[0], ve[1]]).unwrap()
            )
        );
        assert_eq!(reg_query(&[r"HKLM\SOFTWARE\no such key swofty"]), None);
    }
}
