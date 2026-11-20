//! Manifest ops and strings with their `when` conditions and `{{id}}` placeholders resolved from the profile's options.

use serde_json::Value;

use crate::manifest::{AddonManifest, AddonNeed};
use crate::profile::Profile;
use crate::{Error, Result};

use super::OnAddons;

pub fn render_ops(doc: &mut Value, m: &AddonManifest, profile: &Profile, on: &OnAddons) -> Result<()> {
    let fail = |msg: String| Error::Resolve(format!("{}: {msg}", m.id));
    let option = |id: &str| {
        m.options
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| fail(format!("unknown option {id}")))
    };
    let holds = |when: Value| -> Result<bool> {
        let Value::Object(when) = when else {
            return Err(fail("when must be an object of option values and addon".into()));
        };
        let mut hit = true;
        for (id, want) in &when {
            if id == "addon" {
                let needs = AddonNeed::parse_list(want).map_err(|e| fail(format!("when.addon: {e}")))?;
                hit &= needs.iter().all(|n| on.has(n));
                continue;
            }
            let o = option(id)?;
            let want = o
                .normalise(want)
                .ok_or_else(|| fail(format!("when: {want} is not a value of {id}")))?;
            hit &= on.value(profile, m, o) == want;
        }
        Ok(hit)
    };
    let Some(doc) = doc.as_object_mut() else {
        return Ok(());
    };
    let file = doc.remove("when").map(holds).transpose()?.unwrap_or(true);
    let Some(ops) = doc.get_mut("ops").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    if !file {
        ops.clear();
        return Ok(());
    }
    let mut keep = Vec::with_capacity(ops.len());
    for mut op in ops.drain(..) {
        if let Some(when) = op.as_object_mut().and_then(|o| o.remove("when"))
            && !holds(when)?
        {
            continue;
        }
        substitute(&mut op, &|id| {
            let o = option(id)?;
            Ok(o.placeholder(&on.value(profile, m, o)))
        })?;
        keep.push(op);
    }
    *ops = keep;
    Ok(())
}

/// One manifest string with its `{{id}}` placeholders filled from the profile's option values.
pub fn render_text(text: &str, m: &AddonManifest, profile: &Profile, on: &OnAddons) -> Result<String> {
    let mut v = Value::String(text.to_owned());
    substitute(&mut v, &|id| {
        let o = m
            .options
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| Error::Resolve(format!("{}: unknown option {id}", m.id)))?;
        Ok(o.placeholder(&on.value(profile, m, o)))
    })?;
    Ok(v.as_str().expect("a string stays a string").to_owned())
}

pub(super) fn substitute(v: &mut Value, lookup: &dyn Fn(&str) -> Result<String>) -> Result<()> {
    match v {
        Value::String(s) if s.contains("{{") => {
            let mut out = String::with_capacity(s.len());
            let mut rest = s.as_str();
            while let Some(start) = rest.find("{{") {
                let end = rest[start..]
                    .find("}}")
                    .ok_or_else(|| Error::Resolve(format!("unclosed {{{{ in {s}")))?;
                out.push_str(&rest[..start]);
                out.push_str(&lookup(rest[start + 2..start + end].trim())?);
                rest = &rest[start + end + 2..];
            }
            out.push_str(rest);
            *s = out;
        }
        Value::Array(a) => a.iter_mut().try_for_each(|x| substitute(x, lookup))?,
        Value::Object(o) => o.values_mut().try_for_each(|x| substitute(x, lookup))?,
        _ => {}
    }
    Ok(())
}
