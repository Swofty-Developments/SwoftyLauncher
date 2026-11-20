//! The sprites of a build: the art each one draws, the stock files and fill material they share, and the quad
//! slots in use.

use super::model::{Bind, HudElement};
use super::scene::{SpriteInfo, Sprites};
use super::stock::Stock;
use super::textures::Res;
use super::{Input, err};
use super::{dials, model, textures};
use crate::{Error, Result};

pub(super) struct Expanded {
    pub(super) input: usize,
    pub(super) rec: HudElement,
    pub(super) switch: Option<Bind>,
}

pub(super) fn art_bounds(sprite: &model::Sprite, art: &textures::Art) -> [f64; 4] {
    sprite.bounds.unwrap_or_else(|| {
        let ppu = sprite.ppu.unwrap_or(3.0);
        let (w, h) = (art.width as f64 / ppu, art.height as f64 / ppu);
        [-w / 2.0, -h / 2.0, w / 2.0, h / 2.0]
    })
}

pub(super) struct Loaded {
    pub(super) sprites: Sprites,
    pub(super) resources: Vec<Res>,
}

pub(super) fn bind_slots(resources: &mut Vec<Res>, n: &textures::Names, uses_fill: bool, uses_edge: bool) {
    if uses_fill {
        resources.push(textures::binding_resource(n, "central_bg_fill", false));
    }
    if uses_edge {
        resources.push(textures::binding_resource(n, "central_bg_edge", true));
    }
}

/// What every sprite of a build shares: the stock files, the fill material and the quad slots in use.
pub(super) struct SpriteEnv<'a> {
    pub(super) stock: &'a dyn Stock,
    pub(super) fill_material: &'a [u8],
    pub(super) uses_fill: bool,
    pub(super) uses_edge: bool,
}

pub(super) fn load_sprites(
    env: &SpriteEnv,
    input: &Input,
    pfx: &str,
    tinted: &std::collections::BTreeSet<String>,
    team: [u8; 3],
) -> Result<Loaded> {
    let SpriteEnv {
        stock,
        fill_material,
        uses_fill,
        uses_edge,
    } = *env;
    let mut sprites = Sprites::new();
    let mut resources = Vec::new();
    for (name, s) in &input.record.sprites {
        if let Some(tex) = &s.texture {
            let names = textures::stock_names(&format!("{pfx}_"), name, tex);
            let tint = s.tint.as_deref().map(textures::hex_rgba).transpose()?;
            resources.extend(textures::stock_sprite_resources(&names, fill_material, tint)?);
            bind_slots(&mut resources, &names, uses_fill, uses_edge);
            let bounds = match (s.bounds, s.fit) {
                (Some(b), _) => b,
                (None, Some(f)) => textures::fit(textures::texture_size(stock, dials::STOCK_ERP, tex)?, f),
                (None, None) => {
                    return Err(err(format!(
                        "sprite {name}: a texture sprite needs fit or bounds"
                    )));
                }
            };
            sprites.insert(
                name.clone(),
                SpriteInfo {
                    bounds,
                    names,
                    tinted: None,
                },
            );
            continue;
        }
        if s.png.is_empty() {
            return Err(err(format!("sprite {name}: needs png or texture")));
        }
        let path = input.dir.join(&s.png);
        let data = std::fs::read(&path).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
        let mut art = textures::decode_png(&data).map_err(|e| err(format!("{}: {e}", s.png)))?;
        art.check(name)?;
        if let Some(t) = &s.tint {
            art = art.tinted(textures::hex_colour(t)?);
        }
        let key = format!("{pfx}_");
        let names = textures::names(&key, name);
        resources.extend(textures::sprite_resources(&names, &art, fill_material)?);
        bind_slots(&mut resources, &names, uses_fill, uses_edge);
        let pv = if tinted.contains(name) {
            let pv_names = textures::names(&key, &format!("{name}_pv"));
            resources.extend(textures::sprite_resources(
                &pv_names,
                &art.tinted(team),
                fill_material,
            )?);
            resources.push(textures::binding_resource(&pv_names, "central_bg_fill", false));
            Some(pv_names)
        } else {
            None
        };
        sprites.insert(
            name.clone(),
            SpriteInfo {
                bounds: art_bounds(s, &art),
                names,
                tinted: pv,
            },
        );
    }
    Ok(Loaded { sprites, resources })
}

pub(super) fn tinted_sprites(elements: &[model::Element], out: &mut std::collections::BTreeSet<String>) {
    for e in elements {
        if e.tint.is_some() {
            out.extend(e.sprite.clone());
            out.extend(e.fill_sprite.clone());
        }
        tinted_sprites(&e.children, out);
    }
}

pub(super) fn stock_parts(rec: &HudElement) -> Vec<String> {
    if rec.stock_parts.is_empty() {
        dials::STOCK_KIDS.iter().map(|s| (*s).to_owned()).collect()
    } else {
        rec.stock_parts.clone()
    }
}
