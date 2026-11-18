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
