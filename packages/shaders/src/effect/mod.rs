//! `<id>.effect.json` parsing and the generated `swofty/params.hlsli` (DESIGN.md §3).

use std::collections::HashSet;

use serde::Deserialize;

use crate::text::is_ident;
use crate::{
    Effect, MAX_MIPS, MAX_SLOTS, Origin, Param, ParamKind, PassDecl, TextureDecl, Value, mip_levels,
};

#[derive(Deserialize)]
struct RawEffect {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    params: Vec<RawParam>,
    #[serde(default)]
    textures: Vec<TextureDecl>,
    #[serde(default)]
    passes: Vec<PassDecl>,
}

#[derive(Deserialize)]
struct RawParam {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    values: Vec<String>,
    #[serde(default)]
    label: String,
    #[serde(default)]
    doc: String,
}

/// The `#define` suffix of an enum value: upper-cased, non-alphanumerics as `_`.
pub(crate) fn define_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn number_default(raw: &Option<serde_json::Value>, who: &str) -> Result<Option<f64>, String> {
    match raw {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("{who}: default must be a number")),
    }
}

fn vector_default(raw: &Option<serde_json::Value>, n: usize, who: &str) -> Result<Vec<f64>, String> {
    match raw {
        None => Ok(vec![0.0; n]),
        Some(serde_json::Value::Array(a)) if a.len() == n => a
            .iter()
            .map(|v| {
                v.as_f64()
                    .ok_or_else(|| format!("{who}: default must hold {n} numbers"))
            })
            .collect(),
        Some(_) => Err(format!("{who}: default must be an array of {n} numbers")),
    }
}

fn check_range(who: &str, v: f64, min: Option<f64>, max: Option<f64>) -> Result<(), String> {
    if min.is_some_and(|m| v < m) || max.is_some_and(|m| v > m) {
        return Err(format!("{who}: default {v} is outside its min/max"));
    }
    Ok(())
}

fn build_param(raw: RawParam, slot: usize, effect: &str) -> Result<Param, String> {
    let who = format!("effect `{effect}` param `{}`", raw.id);
    if !is_ident(&raw.id) {
        return Err(format!("{who}: the id must match [a-z][a-z0-9_]*"));
    }
    if let (Some(lo), Some(hi)) = (raw.min, raw.max)
        && lo > hi
    {
        return Err(format!("{who}: min {lo} is above max {hi}"));
    }
    let (min, max) = (raw.min, raw.max);
    let (kind, default) = match raw.kind.as_str() {
        "float" => {
            let d = number_default(&raw.default, &who)?
                .unwrap_or_else(|| 0.0f64.max(min.unwrap_or(f64::MIN)).min(max.unwrap_or(f64::MAX)));
            check_range(&who, d, min, max)?;
            (
                ParamKind::Float {
                    min,
                    max,
                    step: raw.step,
                },
                Value::Number(d),
            )
        }
        "int" => {
            let d = number_default(&raw.default, &who)?.unwrap_or(0.0);
            if d.fract() != 0.0 {
                return Err(format!("{who}: an int default must be a whole number"));
            }
            check_range(&who, d, min, max)?;
            let to_int = |v: Option<f64>| -> Result<Option<i64>, String> {
                match v {
                    Some(v) if v.fract() != 0.0 => {
                        Err(format!("{who}: an int's min and max must be whole numbers"))
                    }
                    Some(v) => Ok(Some(v as i64)),
                    None => Ok(None),
                }
            };
            (
                ParamKind::Int {
                    min: to_int(min)?,
                    max: to_int(max)?,
                },
                Value::Number(d),
            )
        }
        "bool" => {
            let d = match &raw.default {
                None => false,
                Some(serde_json::Value::Bool(b)) => *b,
                Some(_) => return Err(format!("{who}: a bool default must be true or false")),
            };
            (ParamKind::Bool, Value::Bool(d))
        }
        "enum" => {
            if raw.values.is_empty() {
                return Err(format!("{who}: an enum needs values"));
            }
            let mut seen = HashSet::new();
            for v in &raw.values {
                if v.is_empty() {
                    return Err(format!("{who}: an enum value is empty"));
                }
                if !seen.insert(define_name(v)) {
                    return Err(format!(
                        "{who}: enum value `{v}` repeats another once upper-cased"
                    ));
                }
            }
            let d = match &raw.default {
                None => raw.values[0].clone(),
                Some(serde_json::Value::String(s)) if raw.values.contains(s) => s.clone(),
                Some(_) => return Err(format!("{who}: the default is not one of the values")),
            };
            (ParamKind::Enum { values: raw.values }, Value::Text(d))
        }
        k @ ("float2" | "color3" | "color4") => {
            let n = match k {
                "float2" => 2,
                "color3" => 3,
                _ => 4,
            };
            let d = vector_default(&raw.default, n, &who)?;
            for &v in &d {
                check_range(&who, v, min, max)?;
            }
            let kind = match n {
                2 => ParamKind::Float2 { min, max },
                3 => ParamKind::Color3 { min, max },
                _ => ParamKind::Color4 { min, max },
            };
            (kind, Value::Vector(d))
        }
        other => return Err(format!("{who}: unknown type `{other}`")),
    };
    Ok(Param {
        label: if raw.label.is_empty() {
            raw.id.clone()
        } else {
            raw.label
        },
        id: raw.id,
        doc: raw.doc,
        kind,
        default,
        slot,
    })
}

/// Reads an `<id>.effect.json` (DESIGN.md §3).
pub fn parse_effect(json: &str, origin: Origin) -> Result<Effect, String> {
    let mut raw: RawEffect = serde_json::from_str(json).map_err(|e| format!("effect: {e}"))?;
    let id = raw.id;
    if !is_ident(&id) {
        return Err(format!("effect id `{id}` must match [a-z][a-z0-9_]*"));
    }
    let mut params = Vec::new();
    let mut seen = HashSet::new();
    let mut defines = HashSet::new();
    let mut slot = 0;
    for p in raw.params {
        if !seen.insert(p.id.clone()) {
            return Err(format!("effect `{id}`: param `{}` is declared twice", p.id));
        }
        let param = build_param(p, slot, &id)?;
        if let ParamKind::Enum { values } = &param.kind {
            for v in values {
                let name = format!("{}_{}", define_name(&param.id), define_name(v));
                if !defines.insert(name.clone()) {
                    return Err(format!("effect `{id}`: enum constant {name} is defined twice"));
                }
            }
        }
        slot += param.kind.width();
        params.push(param);
    }
    if slot > MAX_SLOTS {
        return Err(format!(
            "effect `{id}` needs {slot} param slots; the limit is {MAX_SLOTS}"
        ));
    }

    let mut warnings = Vec::new();
    for t in &mut raw.textures {
        let who = format!("effect `{id}` texture `{}`", t.name);
        if let Some(mips) = t.mips {
            if t.file.is_some() && mips > 1 {
                return Err(format!("{who}: a file texture takes no mips"));
            }
            if mips == 0 {
                return Err(format!("{who}: mips must be at least 1"));
            }
            let limit = match (t.width, t.height) {
                (Some(w), Some(h)) => mip_levels(w, h, MAX_MIPS),
                _ => MAX_MIPS,
            };
            if mips > limit {
                warnings.push(format!(
                    "{who}: mips {mips} is more than the texture allows; using {limit}"
                ));
                t.mips = Some(limit);
            }
        }
    }

    let mut textures = HashSet::new();
    for t in &raw.textures {
        let who = format!("effect `{id}` texture `{}`", t.name);
        if t.name.is_empty() || !t.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("{who}: the name must be letters, digits and `_`"));
        }
        if t.name == "original" || t.name == "previous" {
            return Err(format!("{who}: the name is reserved"));
        }
        if !textures.insert(t.name.as_str()) {
            return Err(format!("{who}: declared twice"));
        }
        if t.width.is_some() != t.height.is_some() {
            return Err(format!("{who}: give both width and height"));
        }
        if t.file.is_some() && (t.scale.is_some() || t.width.is_some() || t.persistent) {
            return Err(format!("{who}: a file texture takes no size or persistence"));
        }
        if t.scale.is_some_and(|s| s <= 0.0) {
            return Err(format!("{who}: the scale must be above 0"));
        }
    }
    if raw.passes.is_empty() {
        return Err(format!("effect `{id}` has no passes"));
    }
    let mut pass_names = HashSet::new();
    for p in &raw.passes {
        let who = format!("effect `{id}` pass `{}`", p.name);
        if !pass_names.insert(p.name.as_str()) {
            return Err(format!("{who}: declared twice"));
        }
        if p.shader.is_empty() {
            return Err(format!("{who}: no shader"));
        }
        if p.inputs.len() > 8 {
            return Err(format!("{who}: more than 8 inputs"));
        }
        for i in &p.inputs {
            if i != "original" && i != "previous" && !textures.contains(i.as_str()) {
                return Err(format!("{who}: input `{i}` is not a declared texture"));
            }
        }
        if let Some(o) = &p.output {
            let decl = raw.textures.iter().find(|t| &t.name == o);
            match decl {
                None => return Err(format!("{who}: output `{o}` is not a declared texture")),
                Some(t) if t.file.is_some() => return Err(format!("{who}: output `{o}` is a file texture")),
                Some(_) => {}
            }
        }
    }
    Ok(Effect {
        name: if raw.name.is_empty() { id.clone() } else { raw.name },
        id,
        description: raw.description,
        params,
        textures: raw.textures,
        passes: raw.passes,
        slots: slot,
        origin,
        warnings,
    })
}

/// The `swofty/params.hlsli` accessors for an effect (DESIGN.md §3).
pub fn params_hlsl(effect: &Effect) -> String {
    let mut out = String::from("#pragma once\n#include \"swofty/postfx.hlsli\"\n\n");
    for p in &effect.params {
        let n = p.slot;
        let id = &p.id;
        match &p.kind {
            ParamKind::Float { .. } => {
                out.push_str(&format!("float param_{id}() {{ return option({n}); }}\n"))
            }
            ParamKind::Int { .. } => out.push_str(&format!(
                "int param_{id}() {{ return int(round(option({n}))); }}\n"
            )),
            ParamKind::Bool => out.push_str(&format!("bool param_{id}() {{ return option({n}) > 0.5; }}\n")),
            ParamKind::Enum { values } => {
                out.push_str(&format!(
                    "uint param_{id}() {{ return uint(round(option({n}))); }}\n"
                ));
                for (k, v) in values.iter().enumerate() {
                    out.push_str(&format!("#define {}_{} {k}\n", define_name(id), define_name(v)));
                }
            }
            ParamKind::Float2 { .. } | ParamKind::Color3 { .. } | ParamKind::Color4 { .. } => {
                let w = p.kind.width();
                let args: Vec<String> = (0..w).map(|i| format!("option({})", n + i)).collect();
                out.push_str(&format!(
                    "float{w} param_{id}() {{ return float{w}({}); }}\n",
                    args.join(", ")
                ));
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests;
