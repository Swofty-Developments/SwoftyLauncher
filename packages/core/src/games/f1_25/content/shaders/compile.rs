//! Compiling the catalog's effects, and the shader files a build writes for the game.

use std::path::PathBuf;

use sha2::{Digest, Sha256};
use swofty_shaders::{CompiledEffect, Origin, Target};

use crate::profile::Profile;
use crate::store::Store;
use crate::{Error, Result, io_err};

use super::live::{ACTIVE_FILE, resolve_active};
use super::{Home, Loaded, library, load};

/// The library extracted to the cache, named by its content.
pub(super) fn extract_library(store: &Store) -> Result<PathBuf> {
    let mut h = Sha256::new();
    for (path, bytes) in library() {
        h.update(path.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    let key = crate::game_folder::baseline::hex(&h.finalize());
    let root = store.shaders_cache_dir().join("library");
    let dir = root.join(&key[..16]);
    if dir.join(".done").is_file() {
        return Ok(dir);
    }
    let tmp = root.join(format!("{}.{}.tmp", &key[..16], std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    for (path, bytes) in library() {
        let to = tmp.join(path);
        let parent = to.parent().expect("has parent");
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
        std::fs::write(&to, bytes).map_err(io_err(&to))?;
    }
    std::fs::write(tmp.join(".done"), b"").map_err(io_err(&tmp))?;
    std::fs::remove_dir_all(&dir).ok();
    if std::fs::rename(&tmp, &dir).is_err() {
        std::fs::remove_dir_all(&tmp).ok();
        if !dir.join(".done").is_file() {
            return Err(Error::Other(format!(
                "{}: could not extract the shader library",
                dir.display()
            )));
        }
    }
    Ok(dir)
}

pub(super) fn decode_image(png: &[u8]) -> std::result::Result<(u32, u32, Vec<u8>), String> {
    let tex = crate::content::postfx::png_to_tex(png)?;
    let word = |at: usize| u32::from_le_bytes(tex[at..at + 4].try_into().expect("four bytes"));
    Ok((word(4), word(8), tex[16..].to_vec()))
}

pub(super) fn tex_bytes(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + rgba.len());
    out.extend_from_slice(b"SWTX");
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(rgba);
    out
}

/// Compiles every catalog effect for `target` (DXIL for the game, SPIR-V for the preview). An
/// effect that fails is left out and named in the second list.
pub fn compile_all(store: &Store, target: Target) -> (Vec<CompiledEffect>, Vec<String>) {
    compile_loaded(store, &load(store), target)
}

pub(super) fn compile_loaded(
    store: &Store,
    loaded: &Loaded,
    target: Target,
) -> (Vec<CompiledEffect>, Vec<String>) {
    let mut warnings = loaded.warnings.clone();
    let Some(dxc) = crate::content::postfx::dxc() else {
        warnings.push("dxc was not found (set SWOFTY_DXC, or put dxc on PATH)".into());
        return (Vec::new(), warnings);
    };
    let library_dir = if loaded.homes.values().any(|h| matches!(h, Home::Library)) {
        match extract_library(store) {
            Ok(d) => Some(d.join("effects")),
            Err(e) => {
                warnings.push(e.to_string());
                None
            }
        }
    } else {
        None
    };
    let cache = store.shaders_cache_dir().join(match target {
        Target::Dxil => "dxil",
        Target::Spirv => "spirv",
    });
    let mut out = Vec::new();
    for effect in &loaded.catalog.effects {
        let dir = match loaded.homes.get(&effect.id) {
            Some(Home::Addon(dir)) => dir.clone(),
            Some(Home::Library) => match &library_dir {
                Some(d) => d.clone(),
                None => {
                    warnings.push(format!(
                        "the effect {} was skipped: the effect library did not unpack",
                        effect.id
                    ));
                    continue;
                }
            },
            None => {
                warnings.push(format!("the effect {} has no source to compile", effect.id));
                continue;
            }
        };
        let blobs =
            match swofty_shaders::compile(effect, &dir, target, &dxc, crate::content::postfx::HEADER, &cache)
            {
                Ok(b) => b,
                Err(e) if target == Target::Spirv && e.contains("SPIR-V CodeGen not available") => {
                    // Every effect fails the same way: say it once.
                    warnings.clear();
                    warnings.push(
                    "This launcher's shader compiler can't build effects for the preview (it has no SPIR-V). \
                     Shaders still work in game; updating Swofty Launcher brings a compiler that can."
                        .to_owned(),
                );
                    return (Vec::new(), warnings);
                }
                Err(e) => {
                    warnings.push(format!("the effect {} did not compile: {e}", effect.id));
                    continue;
                }
            };
        let mut images = Vec::new();
        let mut failed = None;
        for t in &effect.textures {
            let Some(file) = &t.file else { continue };
            let image = std::fs::read(dir.join(file))
                .map_err(|e| e.to_string())
                .and_then(|png| decode_image(&png));
            match image {
                Ok((w, h, rgba)) => images.push((t.name.clone(), w, h, rgba)),
                Err(e) => failed = Some(format!("the effect {}: {file}: {e}", effect.id)),
            }
        }
        if let Some(e) = failed {
            warnings.push(e);
            continue;
        }
        out.push(CompiledEffect {
            effect: effect.clone(),
            blobs,
            images,
        });
    }
    (out, warnings)
}

/// What a build writes under `swofty/` for shaders.
#[derive(Debug, Default)]
pub struct BuildFiles {
    /// Paths relative to `swofty/`.
    pub files: Vec<(String, Vec<u8>)>,
    /// Identifies the content without the live serial, for the build hash.
    pub digest: String,
    pub line: Option<String>,
    pub warnings: Vec<String>,
}

/// `shaders/effects.json`, the compiled passes and textures, and `shaders/active.json` for
/// `profile`. A failed effect or a config with errors is a warning, never a failed build.
pub fn build_files(store: &Store, profile: &Profile) -> BuildFiles {
    let mut out = BuildFiles::default();
    if profile.is_vanilla() {
        return out;
    }
    let loaded = load(store);
    if loaded.catalog.effects.is_empty() {
        out.warnings = loaded.warnings;
        return out;
    }
    let resolved = resolve_active(store, profile, &loaded.catalog);
    if resolved.active.instances.is_empty() {
        // Nothing runs, so the game gets no shader files and the core arms none of its hooks.
        out.warnings = loaded.warnings;
        if let Some(problem) = &resolved.problem {
            out.warnings.push(format!("shaders: {problem}; shaders are off"));
        }
        out.line = Some("shaders: off".into());
        return out;
    }
    let (compiled, mut warnings) = compile_loaded(store, &loaded, Target::Dxil);
    if let Some(problem) = &resolved.problem {
        warnings.push(format!("shaders: {problem}; shaders are off"));
    }
    out.warnings = warnings;
    if compiled.is_empty() {
        out.line = Some("shaders: off".into());
        out.warnings.push(format!(
            "shaders: {} is off; no effect compiled",
            resolved.active.name
        ));
        return out;
    }
    let mut active = resolved.active;
    let known: Vec<&str> = compiled.iter().map(|c| c.effect.id.as_str()).collect();
    active.instances.retain(|i| {
        let ok = known.contains(&i.effect.as_str());
        if !ok {
            out.warnings.push(format!(
                "shaders: {} skips {}, which did not compile",
                active.name, i.effect
            ));
        }
        ok
    });
    let mut effects = Vec::new();
    for c in &compiled {
        let id = &c.effect.id;
        let mut textures = Vec::new();
        for t in &c.effect.textures {
            let image = c.images.iter().find(|i| i.0 == t.name);
            let file = image.map(|(name, w, h, rgba)| {
                let rel = format!("shaders/tex/{id}-{name}.tex");
                out.files.push((rel.clone(), tex_bytes(*w, *h, rgba)));
                rel
            });
            let mut v = serde_json::json!({ "name": t.name });
            let o = v.as_object_mut().expect("object");
            if file.is_some() {
                o.insert("format".into(), "rgba8".into());
            } else {
                o.insert(
                    "format".into(),
                    t.format.clone().unwrap_or_else(|| "rgba16f".into()).into(),
                );
                if let Some(s) = t.scale {
                    o.insert("scale".into(), s.into());
                }
                if let Some(w) = t.width {
                    o.insert("width".into(), w.into());
                }
                if let Some(h) = t.height {
                    o.insert("height".into(), h.into());
                }
                if t.persistent {
                    o.insert("persistent".into(), true.into());
                }
                if let Some(m) = t.mips.filter(|&m| m > 1) {
                    o.insert("mips".into(), m.into());
                }
            }
            if let Some(f) = file {
                o.insert("file".into(), f.into());
            }
            textures.push(v);
        }
        let mut passes = Vec::new();
        for (n, (pass, dxil)) in c.effect.passes.iter().zip(&c.blobs).enumerate() {
            let rel = format!("shaders/fx/{id}-{n:02}.dxil");
            out.files.push((rel.clone(), dxil.clone()));
            let mut v = serde_json::json!({
                "name": pass.name,
                "dxil": rel,
                "inputs": pass.inputs,
                "output": pass.output.clone().unwrap_or_default(),
                "threads": pass.threads.unwrap_or([8, 8]),
                "raw": pass.raw,
                "budget_ms": pass.budget_ms.unwrap_or(1.5),
            });
            if let Some(when) = &pass.when {
                v["when"] = when.clone();
            }
            passes.push(v);
        }
        let addon = match &c.effect.origin {
            Origin::Library => String::new(),
            Origin::Addon { id, .. } => id.clone(),
        };
        effects.push(serde_json::json!({
            "id": id, "name": c.effect.name, "addon": addon, "slots": c.effect.slots,
            "textures": textures, "passes": passes,
        }));
    }
    let effects_json = serde_json::to_vec_pretty(&serde_json::json!({ "version": 1, "effects": effects }))
        .expect("serialisable");
    out.files.push(("shaders/effects.json".into(), effects_json));
    let mut h = Sha256::new();
    for (rel, bytes) in &out.files {
        h.update(rel.as_bytes());
        h.update(bytes);
    }
    let serial = active.serial;
    active.serial = 0;
    h.update(serde_json::to_vec(&active).expect("serialisable"));
    active.serial = serial;
    out.digest = crate::game_folder::baseline::hex(&h.finalize());
    let passes: usize = active
        .instances
        .iter()
        .filter_map(|i| compiled.iter().find(|c| c.effect.id == i.effect))
        .map(|c| c.effect.passes.len())
        .sum();
    out.line = Some(if active.instances.is_empty() {
        "shaders: off".into()
    } else {
        format!(
            "shaders: config \"{}\" runs {} effect(s) ({passes} passes)",
            active.name,
            active.instances.len()
        )
    });
    let mut json = serde_json::to_vec_pretty(&active).expect("serialisable");
    json.push(b'\n');
    out.files.push((ACTIVE_FILE.into(), json));
    out
}
