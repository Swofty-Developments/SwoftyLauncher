use swofty_formats::cube::Cube;

use super::Built;
use super::buildhash::BuildHash;
use super::run::Run;
use crate::addons::settings::{OnAddons, render_text};
use crate::content::runtime_assets::{PostFxLive, PostFxPass, PostFxTexture, PostFxWhen, Runtime};
use crate::content::shaders::BuildFiles;
use crate::manifest::{AddonManifest, Binds, PassValue, PostFx, Strength, placeholders, relative_inside};
use crate::profile::Profile;
use crate::{Error, Result};

/// Grades the picture: the look of the last shader pack in load order, each addon's post-processing
/// textures and passes, and the shader effects of the profile's active config.
pub(super) fn grade(
    run: Run,
    built: &mut Built,
    hash: &mut BuildHash,
    runtime: &mut Runtime,
) -> Result<BuildFiles> {
    look(run, built, hash, runtime)?;
    postfx_textures(run, hash, runtime)?;
    postfx_passes(run, built, hash, runtime)?;
    let mut shader_files = crate::content::shaders::build_files(run.store, run.settings);
    if !shader_files.files.is_empty() {
        runtime.enable("postfx");
    }
    hash.update(format!("shaders {};", shader_files.digest));
    built.lines.extend(shader_files.line.take());
    built.warnings.append(&mut shader_files.warnings);
    Ok(shader_files)
}

fn look(run: Run, built: &mut Built, hash: &mut BuildHash, runtime: &mut Runtime) -> Result<()> {
    let looks: Vec<&AddonManifest> = run
        .order
        .iter()
        .map(|id| &run.addons[id])
        .filter(|m| m.shader.is_some())
        .collect();
    if let Some((m, earlier)) = looks.split_last() {
        for e in earlier {
            built.warnings.push(format!(
                "{}: its look is not applied; one shader look applies at a time, and {} loads after it",
                e.name, m.name
            ));
        }
        let (rel, strength) = shader_look(m, run.settings, run.on)?;
        let path = run.store.addons_dir().join(&m.id).join(&rel);
        let text =
            std::fs::read_to_string(&path).map_err(|e| Error::Other(format!("{}: {rel}: {e}", m.name)))?;
        let cube = Cube::parse(&text).map_err(|e| Error::Other(format!("{}: {rel}: {e}", m.name)))?;
        hash.update(format!("shader {} {rel} {strength};", m.id));
        hash.update(text.as_bytes());
        runtime.shader(cube.pack(), strength / 100.0, &m.name);
        built.lines.push(format!(
            "shaders: {} grades the picture with {rel} ({n}x{n}x{n}) at {strength}% strength",
            m.name,
            n = cube.size
        ));
    }
    Ok(())
}

fn postfx_textures(run: Run, hash: &mut BuildHash, runtime: &mut Runtime) -> Result<()> {
    for id in run.order {
        let m = &run.addons[id];
        if m.postfx.is_empty() {
            continue;
        }
        for t in &m.postfx_textures {
            let image = match &t.file {
                None => None,
                Some(file) => {
                    let path = run.store.addons_dir().join(&m.id).join(file);
                    let png =
                        std::fs::read(&path).map_err(|e| Error::Other(format!("{}: {file}: {e}", m.name)))?;
                    hash.update(format!("postfx texture {} {file};", m.id));
                    hash.update(&png);
                    Some(
                        crate::content::postfx::png_to_tex(&png)
                            .map_err(|e| Error::Other(format!("{}: {file}: {e}", m.name)))?,
                    )
                }
            };
            hash.update(format!("postfx texture {} {t:?};", m.id));
            runtime.postfx_texture(
                PostFxTexture {
                    name: texture_name(m, &t.name),
                    scale: t.scale,
                    width: t.width,
                    height: t.height,
                    format: if image.is_some() {
                        "rgba8".into()
                    } else {
                        t.format.clone().unwrap_or_else(|| "rgba16f".into())
                    },
                    persistent: image.is_none().then_some(t.persistent),
                    file: None,
                },
                image,
            );
        }
        if let Some(t) = &m.postfx_toggle
            && let [option] = placeholders(t)[..]
        {
            hash.update(format!("postfx toggle {} {option};", m.id));
            runtime.postfx_toggle(&m.id, option);
        }
    }
    Ok(())
}

fn postfx_passes(run: Run, built: &mut Built, hash: &mut BuildHash, runtime: &mut Runtime) -> Result<()> {
    let mut passes: Vec<(i32, usize, &AddonManifest, &PostFx)> = Vec::new();
    for id in run.order {
        for pass in &run.addons[id].postfx {
            passes.push((pass.order, passes.len(), &run.addons[id], pass));
        }
    }
    passes.sort_by_key(|(order, at, ..)| (*order, *at));
    for (_, _, m, pass) in passes {
        let label = pass.label();
        let Some(enabled) = pass_enabled(m, pass, run.settings, run.on)? else {
            hash.update(format!("postfx {} {} off;", m.id, pass.shader));
            built
                .lines
                .push(format!("postfx: {}'s pass {label} is switched off", m.name));
            continue;
        };
        let options = pass_options(m, pass, run.settings, run.on)?;
        let source = run.store.addons_dir().join(&m.id).join(&pass.shader);
        let budget = pass.budget_ms.unwrap_or(1.5);
        let spec = PostFxPass {
            shader: String::new(),
            name: label.clone(),
            addon: m.name.clone(),
            options,
            budget_ms: budget,
            addon_id: m.id.clone(),
            live: pass_live(m, pass),
            inputs: pass
                .inputs
                .iter()
                .map(|n| match n.as_str() {
                    "original" | "previous" => n.clone(),
                    _ => texture_name(m, n),
                })
                .collect(),
            output: pass
                .output
                .as_deref()
                .map(|n| texture_name(m, n))
                .unwrap_or_default(),
            threads: pass.threads.unwrap_or([8, 8]),
            raw: pass.raw,
            when: pass.when.as_ref().map(|w| PostFxWhen {
                views: w.views.clone(),
                skip: w.skip.clone(),
            }),
            enabled,
        };
        hash.update(format!(
            "postfx {} {} {} {spec:?};",
            m.id, pass.shader, pass.order
        ));
        let entry = crate::content::postfx::Entry::of(pass);
        match crate::content::postfx::compile(
            &source,
            &run.store.addon_cache_dir(&m.id).join("postfx"),
            entry,
        ) {
            Ok(crate::content::postfx::Compiled { dxil, .. }) => {
                hash.update(&dxil);
                built.lines.push(format!(
                    "postfx: {}'s pass {label} runs as pass {} before the HUD ({} bytes of DXIL)",
                    m.name,
                    runtime.assets.postfx.len() + 1,
                    dxil.len()
                ));
                runtime.postfx(dxil, spec);
            }
            Err(e) => {
                hash.update(format!("skipped {e};"));
                built
                    .warnings
                    .push(format!("{}: pass {label} is skipped: {e}", m.name));
            }
        }
    }
    Ok(())
}

/// The `.cube` a shader pack's settings pick, and its strength from 0 to 100.
fn shader_look(m: &AddonManifest, p: &Profile, on: &OnAddons) -> Result<(String, f64)> {
    let s = m.shader.as_ref().expect("a shader pack");
    let rel = render_text(&s.lut, m, p, on)?;
    if !relative_inside(&rel) {
        return Err(Error::Other(format!(
            "{}: shader lut {rel} must be a relative path inside the addon",
            m.name
        )));
    }
    let strength = match &s.strength {
        None => 100.0,
        Some(Strength::Percent(v)) => *v,
        Some(Strength::Option(t)) => render_text(t, m, p, on)?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| (0.0..=100.0).contains(v))
            .ok_or_else(|| Error::Other(format!("{}: shader strength {t} is not from 0 to 100", m.name)))?,
    };
    Ok((rel, strength))
}

fn texture_name(m: &AddonManifest, name: &str) -> String {
    format!("{}/{name}", m.id)
}

fn option_id(t: &str) -> Option<&str> {
    match placeholders(t)[..] {
        [id] => Some(id),
        _ => None,
    }
}

/// The slots of a pass that bind to native options, which the core reads live.
fn pass_live(m: &AddonManifest, pass: &PostFx) -> Vec<PostFxLive> {
    pass.options
        .iter()
        .enumerate()
        .filter_map(|(slot, v)| {
            let PassValue::Option(t) = v else { return None };
            let id = option_id(t)?;
            m.options
                .iter()
                .any(|o| o.id == id && o.binds == Binds::Native)
                .then(|| PostFxLive {
                    slot,
                    option: id.into(),
                })
        })
        .collect()
}

/// `None` when the pass is left out; otherwise the live toggle that gates it, if any.
fn pass_enabled(
    m: &AddonManifest,
    pass: &PostFx,
    p: &Profile,
    on: &OnAddons,
) -> Result<Option<Option<String>>> {
    let Some(t) = &pass.enabled else {
        return Ok(Some(None));
    };
    let id = option_id(t)
        .ok_or_else(|| Error::Other(format!("{}: postfx enabled {t} is not one option", m.name)))?;
    if m.options.iter().any(|o| o.id == id && o.binds == Binds::Native) {
        return Ok(Some(Some(id.into())));
    }
    Ok(match render_text(t, m, p, on)?.trim() {
        "true" => Some(None),
        _ => None,
    })
}

/// The values a pass reads with `option(i)`, with `{{id}}` options filled from the profile's settings.
fn pass_options(m: &AddonManifest, pass: &PostFx, p: &Profile, on: &OnAddons) -> Result<Vec<f64>> {
    pass.options
        .iter()
        .map(|v| match v {
            PassValue::Number(n) => Ok(*n),
            PassValue::Option(t) => {
                let text = render_text(t, m, p, on)?;
                match text.trim() {
                    "true" => Ok(1.0),
                    "false" => Ok(0.0),
                    v => v.parse::<f64>().map_err(|_| {
                        Error::Other(format!("{}: postfx option {t} is {v:?}, not a number", m.name))
                    }),
                }
            }
        })
        .collect()
}
