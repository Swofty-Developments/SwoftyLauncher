//! Keeping secrets out of a bundle: the setting names and values that hold tokens and keys.

use serde_json::Value;

use super::REDACTED;

/// A field or variable name that may hold a secret.
pub fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const PARTS: [&str; 10] = [
        "token",
        "secret",
        "passw",
        "passphrase",
        "key",
        "credential",
        "cookie",
        "private",
        "webhook",
        "bearer",
    ];
    PARTS.iter().any(|p| n.contains(p))
        || n == "auth"
        || n.starts_with("auth_")
        || n.ends_with("_auth")
        || n.contains("_auth_")
        || n.contains("oauth")
}

/// `data` with the `t=` token of every page this machine serves (`http://127.0.0.1` or `http://localhost`)
/// replaced, such as League Stewarding's, whose token opens the page. `None` when there is no such token.
pub fn redact_page_tokens(data: &[u8]) -> Option<Vec<u8>> {
    const HOSTS: [&[u8]; 2] = [b"http://127.0.0.1", b"http://localhost"];
    let mut out = Vec::new();
    let mut copied = 0;
    let mut i = 0;
    while i < data.len() {
        let rest = &data[i..];
        if !HOSTS
            .iter()
            .any(|h| rest.len() >= h.len() && rest[..h.len()].eq_ignore_ascii_case(h))
        {
            i += 1;
            continue;
        }
        let end = rest
            .iter()
            .position(|&b| b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'<' | b'>' | b'`'))
            .map_or(data.len(), |n| i + n);
        let mut j = i;
        while j + 3 <= end {
            if !(matches!(data[j], b'?' | b'&') && data[j + 1] == b't' && data[j + 2] == b'=') {
                j += 1;
                continue;
            }
            let value = j + 3;
            let value_end = data[value..end]
                .iter()
                .position(|&b| matches!(b, b'&' | b'#'))
                .map_or(end, |n| value + n);
            if value_end > value {
                out.extend_from_slice(&data[copied..value]);
                out.extend_from_slice(REDACTED.as_bytes());
                copied = value_end;
            }
            j = value_end;
        }
        i = end.max(i + 1);
    }
    (copied > 0).then(|| {
        out.extend_from_slice(&data[copied..]);
        out
    })
}

pub(super) fn secret_value(value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    v.contains("/api/webhooks/") || v.contains("discord.com/api") && v.contains("token")
}

/// Replace every secret-looking value in a JSON document; returns how many.
pub fn redact_json(v: &mut Value) -> usize {
    match v {
        Value::Object(map) => map
            .iter_mut()
            .map(|(k, v)| {
                if secret_name(k) && !v.is_null() {
                    *v = Value::String(REDACTED.into());
                    1
                } else {
                    redact_json(v)
                }
            })
            .sum(),
        Value::Array(items) => items.iter_mut().map(redact_json).sum(),
        Value::String(s) if secret_value(s) => {
            *s = REDACTED.into();
            1
        }
        _ => 0,
    }
}

/// `NAME=value` lines with a secret-looking name or value get `NAME=[redacted]`.
pub fn redact_env(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let end = &line[body.len()..];
        match body.split_once('=') {
            Some((name, value)) if secret_name(name) || secret_value(value) => {
                out.push_str(name);
                out.push('=');
                out.push_str(REDACTED);
                out.push_str(end);
            }
            _ => out.push_str(line),
        }
    }
    out
}
