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

impl Key {
    /// The value named `name`, the case ignored as the registry ignores it.
    pub fn value(&self, name: &str) -> Option<&Data> {
        self.values
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, d)| d)
    }

    /// A REG_SZ or REG_EXPAND_SZ value's text.
    pub fn text(&self, name: &str) -> Option<String> {
        match self.value(name)? {
            Data::Sz(s) | Data::ExpandSz(s) => Some(s.clone()),
            _ => None,
        }
    }

    pub fn dword(&self, name: &str) -> Option<u32> {
        match self.value(name)? {
            Data::Dword(n) => Some(*n),
            _ => None,
        }
    }

    /// A REG_BINARY value's bytes, or a REG_QWORD's eight little-endian bytes.
    pub fn bytes(&self, name: &str) -> Option<Vec<u8>> {
        match self.value(name)? {
            Data::Binary(b) => Some(b.clone()),
            Data::Qword(n) => Some(n.to_le_bytes().to_vec()),
            _ => None,
        }
    }
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

/// `raw` as the UTF-16 code units Windows keeps text in, a trailing odd byte left out.
fn units(raw: &[u8]) -> Vec<u16> {
    raw.chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// UTF-16 text up to its first NUL. Every character reads as Windows keeps it, outside ASCII too; an
/// unpaired surrogate, which no string can hold, reads as U+FFFD where reg.exe prints a `?`.
fn utf16(raw: &[u8]) -> String {
    let wide = units(raw);
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// The data a value of type `kind` holds in `raw`, as `reg query` reads it.
pub fn data(kind: u32, raw: &[u8]) -> Data {
    match kind {
        1 => Data::Sz(utf16(raw)),
        2 => Data::ExpandSz(utf16(raw)),
        3 => Data::Binary(raw.to_vec()),
        4 if raw.len() >= 4 => Data::Dword(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])),
        7 => {
            let wide = units(raw);
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

/// The key, read as `reg query` with `ask` reads it; None when it cannot be opened.
pub fn read(key: &str, ask: Ask) -> Option<Key> {
    open(key, ask).ok().flatten()
}

/// The key read with `ask`, None when it does not exist, or the error Windows gave for opening it.
#[cfg(windows)]
pub fn open(key: &str, ask: Ask) -> Result<Option<Key>, i32> {
    let Some((name, handle, rest)) = hive(key) else {
        return Err(win32::ERROR_INVALID_PARAMETER);
    };
    let path = match rest.is_empty() {
        true => name.to_owned(),
        false => format!("{name}\\{rest}"),
    };
    match win32::Open::at(win32::predefined(handle), rest, ask.view32) {
        Ok(open) => Ok(Some(win32::key(&open, path, ask))),
        Err(win32::ERROR_FILE_NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(not(windows))]
pub fn open(_: &str, _: Ask) -> Result<Option<Key>, i32> {
    Ok(None)
}

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    use super::{Ask, Key};

    const KEY_READ: u32 = 0x2_0019;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    const ERROR_SUCCESS: i32 = 0;
    pub const ERROR_FILE_NOT_FOUND: i32 = 2;
    pub const ERROR_INVALID_PARAMETER: i32 = 87;
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
        pub fn at(parent: *mut c_void, sub: &str, view32: bool) -> Result<Self, i32> {
            let wide: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
            let view = if view32 { KEY_WOW64_32KEY } else { KEY_WOW64_64KEY };
            let mut handle = std::ptr::null_mut();
            // SAFETY: `wide` is NUL-terminated and outlives the call; `handle` is written on success only.
            let status = unsafe { RegOpenKeyExW(parent, wide.as_ptr(), 0, KEY_READ | view, &mut handle) };
            match status {
                ERROR_SUCCESS => Ok(Self { handle, view32 }),
                e => Err(e),
            }
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
                    let child = Open::at(open.handle, sub, open.view32).ok()?;
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
    fn text_outside_ascii_reads_exactly_and_an_unpaired_surrogate_as_a_replacement() {
        let le = |units: &[u16]| -> Vec<u8> { units.iter().flat_map(|u| u.to_le_bytes()).collect() };
        let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain([0]).collect() };
        assert_eq!(
            utf16(&le(&wide("Microsoft® Corporation"))),
            "Microsoft® Corporation"
        );
        assert_eq!(
            data(1, &le(&wide(r"C:\Users\Zoë\Yeni klasör 🏁"))),
            sz(r"C:\Users\Zoë\Yeni klasör 🏁")
        );
        assert_eq!(utf16(&le(&[0x46, 0xD83C, 0x31, 0, 0x58])), "F\u{FFFD}1");
        assert_eq!(utf16(&le(&[0xDC00, 0x41])), "\u{FFFD}A");
        assert_eq!(
            data(2, &le(&[0x25, 0xD800, 0xD800, 0xDE00])),
            Data::ExpandSz("%\u{FFFD}\u{10200}".into())
        );
        assert_eq!(
            data(7, &le(&[0x61, 0xD800, 0, 0xAE, 0, 0])),
            Data::MultiSz(vec!["a\u{FFFD}".into(), "®".into()])
        );
        assert_eq!(utf16(&[0x41, 0, 0x42]), "A");
    }

    /// The keys two `reg query` listings of the same key hold, compared value by value as the parsers see
    /// them. reg.exe writes text in the console's code page, which may not hold every character, and
    /// writes a near letter or a `?` for one it lacks (`®` as `r`). So a key path, value name or value
    /// outside ASCII on either side only has to be there on both and read in process without U+FFFD;
    /// the rest have to match.
    fn listings_agree(here: &str, tool: &str) -> Result<usize, String> {
        use crate::platform::ea_app::reg_tree;
        let blocks = |out: &str| -> Vec<(String, Vec<(String, String)>)> {
            reg_tree(out)
                .into_iter()
                .map(|(path, values)| (path, parse_reg_query(&values)))
                .filter(|(_, values)| !values.is_empty())
                .collect()
        };
        let (here, tool) = (blocks(here), blocks(tool));
        if here.len() != tool.len() {
            return Err(format!(
                "{} keys in process, {} from reg.exe",
                here.len(),
                tool.len()
            ));
        }
        let agree = |ours: &str, theirs: &str, same: fn(&str, &str) -> bool| match ours.is_ascii()
            && theirs.is_ascii()
        {
            true => same(ours, theirs),
            false => !ours.contains('\u{FFFD}'),
        };
        for ((path, values), (tool_path, tool_values)) in here.iter().zip(&tool) {
            if !agree(path, tool_path, |a, b| a.eq_ignore_ascii_case(b)) {
                return Err(format!("{path} in process, {tool_path} from reg.exe"));
            }
            if values.len() != tool_values.len() {
                return Err(format!(
                    "{path}: {values:?} in process, {tool_values:?} from reg.exe"
                ));
            }
            for ((name, data), (tool_name, tool_data)) in values.iter().zip(tool_values) {
                if !agree(name, tool_name, |a, b| a == b) || !agree(data, tool_data, |a, b| a == b) {
                    return Err(format!(
                        "{path}: {name} = {data:?} in process, {tool_name} = {tool_data:?} from reg.exe"
                    ));
                }
            }
        }
        Ok(here.len())
    }

    #[test]
    fn listings_agree_where_reg_exe_writes_a_letter_its_code_page_lacks_another_way() {
        let key = r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
        let listing = |id: &str, publisher: &str, version: &str| {
            format!(
                "\r\n{key}\\{id}\r\n    DisplayVersion    REG_SZ    {version}\r\n    Publisher    REG_SZ    \
                 {publisher}\r\n    DisplayName    REG_SZ    Azure Cosmos DB Emulator\r\n\r\n"
            )
        };
        let id = "{CB0238CA-3B36-47AB-86FD-D60F4C47607A}";
        let here = listing(id, "Microsoft® Corporation", "2.14.28");
        assert_eq!(
            listings_agree(&here, &listing(id, "Microsoftr Corporation", "2.14.28")),
            Ok(1)
        );
        assert_eq!(
            listings_agree(&here, &listing(id, "Microsoft? Corporation", "2.14.28")),
            Ok(1)
        );
        assert_eq!(listings_agree(&here, &here), Ok(1));
        assert_eq!(
            listings_agree(&listing("F1® 25", "EA", "1"), &listing("F1r 25", "EA", "1")),
            Ok(1)
        );

        assert!(listings_agree(&here, &listing(id, "Microsoftr Corporation", "2.14.29")).is_err());
        assert!(listings_agree(&listing("{A}", "EA", "1"), &listing("{B}", "EA", "1")).is_err());
        let replaced = listing(id, "Microsoft\u{FFFD} Corporation", "2.14.28");
        assert!(listings_agree(&replaced, &listing(id, "Microsoft? Corporation", "2.14.28")).is_err());
        let without = format!(
            "\r\n{key}\\{id}\r\n    DisplayVersion    REG_SZ    2.14.28\r\n    DisplayName    REG_SZ    \
             Azure Cosmos DB Emulator\r\n\r\n"
        );
        assert!(listings_agree(&without, &listing(id, "Microsoftr Corporation", "2.14.28")).is_err());
        assert!(listings_agree(&format!("{here}{here}"), &here).is_err());
    }

    /// The text of `key`'s value `name`, or why there is none: the value's type and data when it is not
    /// text, every value the key holds when it has none of that name.
    fn text_or_why(key: &Key, name: &str) -> Result<String, String> {
        if let Some(text) = key.text(name) {
            return Ok(text);
        }
        Err(match key.value(name) {
            Some(data) => format!("{} has {name} as {data:?}", key.path),
            None => format!(
                "{} has no value named {name}; it holds {:?}",
                key.path, key.values
            ),
        })
    }

    #[test]
    fn a_value_reads_as_text_whatever_its_name_s_case_and_without_its_nuls() {
        let le = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        let key = Key {
            path: r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs".into(),
            values: vec![
                ("DllDirectory".into(), data(2, &le("%SystemRoot%\\system32\0"))),
                ("KERNEL32".into(), data(1, &le("kernel32.dll\0\0"))),
                ("user32".into(), data(1, &le("user32.dll"))),
                ("gdi32".into(), data(1, &le("gdi32.dll\0left over"))),
                ("List".into(), data(7, &le("a\0b\0\0"))),
            ],
            ..Key::default()
        };
        assert_eq!(text_or_why(&key, "kernel32").as_deref(), Ok("kernel32.dll"));
        assert_eq!(key.text("Kernel32").as_deref(), Some("kernel32.dll"));
        assert_eq!(
            key.text("dlldirectory").as_deref(),
            Some(r"%SystemRoot%\system32")
        );
        assert_eq!(key.text("USER32").as_deref(), Some("user32.dll"));
        assert_eq!(key.text("Gdi32").as_deref(), Some("gdi32.dll"));
        let list = text_or_why(&key, "list").unwrap_err();
        assert!(
            list.ends_with(r#"KnownDLLs has list as MultiSz(["a", "b"])"#),
            "{list}"
        );
        let missing = text_or_why(&key, "ntdll").unwrap_err();
        assert!(missing.contains("has no value named ntdll"), "{missing}");
        assert!(
            missing.contains(r#"("KERNEL32", Sz("kernel32.dll"))"#),
            "{missing}"
        );
    }

    #[test]
    fn values_read_by_name_whatever_the_case_and_by_their_type() {
        let key = Key {
            path: r"HKEY_LOCAL_MACHINE\SOFTWARE\x".into(),
            values: vec![
                ("DisplayName".into(), sz("Defender")),
                ("Path".into(), Data::ExpandSz(r"%SystemRoot%\x".into())),
                ("STATE".into(), Data::Dword(0x61100)),
                ("MitigationOptions".into(), Data::Qword(0x1000_0000_0000_0000)),
                ("Raw".into(), Data::Binary(vec![1, 2, 3])),
            ],
            ..Key::default()
        };
        assert_eq!(key.text("displayname").as_deref(), Some("Defender"));
        assert_eq!(key.text("PATH").as_deref(), Some(r"%SystemRoot%\x"));
        assert_eq!(key.text("State"), None);
        assert_eq!(key.dword("state"), Some(0x61100));
        assert_eq!(key.dword("DisplayName"), None);
        assert_eq!(
            key.bytes("mitigationoptions"),
            Some(vec![0, 0, 0, 0, 0, 0, 0, 0x10])
        );
        assert_eq!(key.bytes("raw"), Some(vec![1, 2, 3]));
        assert_eq!(key.bytes("missing"), None);
        assert_eq!(
            data(11, &0x1000_0000_0000_0000u64.to_le_bytes()),
            Data::Qword(0x1000_0000_0000_0000)
        );
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

    /// The in-process read and reg.exe agree on keys every Windows has, as [`listings_agree`] holds them.
    #[cfg(windows)]
    #[test]
    fn the_in_process_read_agrees_with_reg_exe() {
        for args in [
            &[r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion"][..],
            &[r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/reg:64"][..],
            &[r"HKCU\Environment"][..],
            &[r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", "/s"][..],
            &[
                r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
                "/s",
            ][..],
            &[r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs"][..],
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
            match listings_agree(&here, &tool) {
                Ok(keys) => assert!(keys > 0, "{args:?}"),
                Err(e) => panic!("{args:?}: {e}"),
            }
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
        assert_eq!(
            open(r"HKLM\SOFTWARE\no such key swofty", Ask::default()),
            Ok(None)
        );
        let session = open(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager",
            Ask::default(),
        )
        .unwrap()
        .unwrap();
        assert!(
            session
                .subkeys
                .iter()
                .any(|s| s.eq_ignore_ascii_case("KnownDLLs"))
        );
        let known = open(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs",
            Ask::default(),
        )
        .unwrap()
        .unwrap();
        let name = if known.value("*kernel32").is_some() {
            "*kernel32"
        } else {
            "kernel32"
        };
        match text_or_why(&known, name) {
            Ok(text) => assert!(
                text.eq_ignore_ascii_case("kernel32.dll"),
                "{} has kernel32 as {text:?}",
                known.path
            ),
            Err(why) => panic!("{why}"),
        }
    }
}
