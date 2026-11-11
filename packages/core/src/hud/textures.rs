//! PNG sprites to the four resources each quad needs: a BC7 surface, its view, a material that
//! links the texture and a material binding for the quad mesh's slot.
//!
//! The surface header, view and binding layouts follow `native/src/core/apply/content/garage_build.hpp`
//! and Halo's `resources.py` (formats and game facts). TODO(stream F): `surface.rs`, `emb.rs` and
//! `material.rs` of `swofty-formats` replace the small writers in this file once they land.

use std::collections::BTreeMap;

use swofty_formats::bc7_enc;

use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

const FMT_BC7_SRGB: u32 = 70;

/// One decoded sprite.
#[derive(Debug, Clone)]
pub struct Art {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn decode_png(data: &[u8]) -> Result<Art> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| err(format!("png: {e}")))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| err("png: too large"))?];
    let info = reader.next_frame(&mut buf).map_err(|e| err(format!("png: {e}")))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return Err(err("png: indexed colour is not expanded")),
    };
    Ok(Art {
        width: info.width,
        height: info.height,
        rgba,
    })
}

impl Art {
    pub fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Self {
        Self {
            width: w,
            height: h,
            rgba: rgba.iter().copied().cycle().take((w * h * 4) as usize).collect(),
        }
    }

    pub fn tinted(&self, colour: [u8; 3]) -> Self {
        let mut out = self.clone();
        for p in out.rgba.chunks_exact_mut(4) {
            for c in 0..3 {
                p[c] = (p[c] as u32 * colour[c] as u32 / 255) as u8;
            }
        }
        out
    }

    pub fn check(&self, name: &str) -> Result<()> {
        if self.width == 0 || self.height == 0 || self.width % 4 != 0 || self.height % 4 != 0 {
            return Err(err(format!(
                "sprite {name}: {}x{} is not a multiple of 4 on both sides",
                self.width, self.height
            )));
        }
        Ok(())
    }
}

pub fn hex_colour(s: &str) -> Result<[u8; 3]> {
    let h = s.trim_start_matches('#');
    if h.len() < 6 {
        return Err(err(format!("colour {s}")));
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| err(format!("colour {s}")));
    Ok([b(0)?, b(2)?, b(4)?])
}

pub fn surface(art: &Art) -> Result<(Vec<u8>, Vec<u8>)> {
    let pixels = bc7_enc::encode_rgba(&art.rgba, art.width as usize, art.height as usize)?;
    let mut head = Vec::with_capacity(40);
    for v in [2, 0x108, FMT_BC7_SRGB, art.width, art.height, 1, 1, 1, 0, 0] {
        head.extend_from_slice(&u32::to_le_bytes(v));
    }
    Ok((head, pixels))
}

pub fn srv(tex: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [5, FMT_BC7_SRGB, 0, 1, 0, 0] {
        out.extend_from_slice(&u32::to_le_bytes(v));
    }
    out.extend_from_slice(tex.as_bytes());
    out.extend_from_slice(b".image\0");
    out
}

/// A `GfxMatBindRes`: u32 slots, u32 unique materials, u32 material block offset (from 0x0c), then
/// per slot its name offset and material offset, the slot names, the material paths.
pub fn emb(slot: &str, material: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [1u32, 1, 8 + slot.len() as u32 + 1] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(slot.as_bytes());
    out.push(0);
    out.extend_from_slice(material.as_bytes());
    out.push(0);
    out
}

const CONST: u32 = 1;
const TEXTURE: u32 = 2;
const HEAP: (u32, u32) = (6, 3);

#[derive(Debug, Clone)]
struct Param {
    name: String,
    kind: u32,
    value: Vec<u8>,
    text: String,
    ty: u32,
    class: u32,
    cbuffer: u32,
    offset: u32,
    link: Option<String>,
}

struct Material {
    shader: String,
    params: Vec<Param>,
    cbuffers: Vec<(String, u32)>,
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| err("material is truncated"))
}

fn cstr(b: &[u8], o: usize) -> Result<String> {
    let rest = b.get(o..).ok_or_else(|| err("material is truncated"))?;
    let end = rest.iter().position(|c| *c == 0).ok_or_else(|| err("material string"))?;
    Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
}

fn parse_material(b: &[u8]) -> Result<Material> {
    let h: Vec<u32> = (0..8).map(|i| u32_at(b, i * 4)).collect::<Result<_>>()?;
    let (n, ncb, vsz, o_sh, o_nm, o_t1, o_t2, o_cb) = (
        h[0] as usize,
        h[1] as usize,
        h[2] as usize,
        h[3] as usize,
        h[4] as usize,
        h[5] as usize,
        h[6] as usize,
        h[7] as usize,
    );
    let base = 0x20 + 36 * n;
    let mut params = Vec::new();
    for i in 0..n {
        let r: Vec<u32> = (0..9)
            .map(|k| u32_at(b, 0x20 + 36 * i + 4 * k))
            .collect::<Result<_>>()?;
        let name = cstr(b, base + o_nm + r[0] as usize)?;
        let mut p = Param {
            name,
            kind: r[1],
            value: Vec::new(),
            text: String::new(),
            ty: r[2],
            class: r[3],
            cbuffer: r[5],
            offset: r[6],
            link: None,
        };
        match r[1] {
            CONST => {
                let at = r[7] as usize;
                p.value = b
                    .get(base + at..base + at + r[4] as usize)
                    .ok_or_else(|| err("material value outside its blob"))?
                    .to_vec();
                if (r[2], r[3]) == HEAP {
                    p.link = Some(cstr(b, base + o_t2 + r[8] as usize)?);
                }
            }
            TEXTURE => p.text = cstr(b, base + o_t1 + r[7] as usize)?,
            k => return Err(err(format!("material record kind {k}"))),
        }
        params.push(p);
    }
    let mut cbuffers = Vec::new();
    for i in 0..ncb {
        let at = base + vsz + 8 * i;
        cbuffers.push((cstr(b, base + o_cb + u32_at(b, at)? as usize)?, u32_at(b, at + 4)?));
    }
    Ok(Material {
        shader: cstr(b, base + o_sh)?,
        params,
        cbuffers,
    })
}

#[derive(Default)]
struct Pool {
    buf: Vec<u8>,
    seen: BTreeMap<String, u32>,
}

impl Pool {
    fn add(&mut self, s: &str) -> u32 {
        if let Some(o) = self.seen.get(s) {
            return *o;
        }
        let o = self.buf.len() as u32;
        self.seen.insert(s.to_owned(), o);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        o
    }
}

fn write_material(m: &Material) -> Vec<u8> {
    let (mut names, mut list1, mut list2, mut cbn) =
        (Pool::default(), Pool::default(), Pool::default(), Pool::default());
    let mut blob = Vec::new();
    let mut recs: Vec<[u32; 9]> = Vec::new();
    for p in &m.params {
        let nm = names.add(&p.name);
        if p.kind == CONST {
            let link = match &p.link {
                Some(l) if (p.ty, p.class) == HEAP => list2.add(l),
                _ => 0,
            };
            recs.push([nm, CONST, p.ty, p.class, p.value.len() as u32, p.cbuffer, p.offset, blob.len() as u32, link]);
            blob.extend_from_slice(&p.value);
        } else {
            let at = list1.add(&p.text);
            recs.push([nm, TEXTURE, p.ty, p.class, p.text.len() as u32 + 1, p.cbuffer, p.offset, at, 0]);
        }
    }
    let mut cbt = Vec::new();
    for (name, size) in &m.cbuffers {
        cbt.extend_from_slice(&cbn.add(name).to_le_bytes());
        cbt.extend_from_slice(&size.to_le_bytes());
    }
    let mut shader = m.shader.clone().into_bytes();
    shader.push(0);
    let o_sh = blob.len() + cbt.len();
    let o_nm = o_sh + shader.len();
    let o_t1 = o_nm + names.buf.len();
    let o_t2 = o_t1 + list1.buf.len();
    let o_cb = o_t2 + list2.buf.len();
    let mut out = Vec::new();
    for v in [recs.len(), m.cbuffers.len(), blob.len(), o_sh, o_nm, o_t1, o_t2, o_cb] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    for r in &recs {
        for v in r {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&blob);
    out.extend_from_slice(&cbt);
    out.extend_from_slice(&shader);
    out.extend_from_slice(&names.buf);
    out.extend_from_slice(&list1.buf);
    out.extend_from_slice(&list2.buf);
    out.extend_from_slice(&cbn.buf);
    out
}

/// Re-encodes a material the way it is stored; equal to the input for every stock material.
pub fn reencode_material(stock: &[u8]) -> Result<Vec<u8>> {
    Ok(write_material(&parse_material(stock)?))
}

/// The stock fill material with white colour constants and its diffuse texture linked to `tex`.
pub fn material(stock: &[u8], tex: &str) -> Result<Vec<u8>> {
    let mut m = parse_material(stock)?;
    let mut linked = (false, false);
    for p in &mut m.params {
        match p.name.as_str() {
            "me_colour" | "me_finalColourTint" if p.kind == CONST && p.value.len() == 16 => {
                p.value = [1.0f32; 4].iter().flat_map(|f| f.to_le_bytes()).collect();
            }
            "me_diffuseTexHeapIndex" => {
                p.link = Some(tex.to_owned());
                linked.0 = true;
            }
            "me_diffuseTex" if p.kind == TEXTURE => {
                p.text = tex.to_owned();
                linked.1 = true;
            }
            _ => {}
        }
    }
    if linked != (true, true) {
        return Err(err("the stock fill material has no me_diffuseTex and me_diffuseTexHeapIndex"));
    }
    let out = write_material(&m);
    let back = parse_material(&out)?;
    let ok = back
        .params
        .iter()
        .any(|p| p.name == "me_diffuseTex" && p.text == tex);
    if !ok {
        return Err(err("the material did not keep its texture link"));
    }
    Ok(out)
}

/// The names of one sprite's resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    pub tex: String,
    pub material: String,
    pub emb: String,
    pub emb_edge: String,
}

pub fn names(prefix: &str, sprite: &str) -> Names {
    let key = format!("{prefix}{sprite}");
    Names {
        tex: format!("eaid://ui_package/art/osd/textures/hud/{key}.tif"),
        material: format!("eaid://ui_package/art/osd/materials/hud/{key}.material"),
        emb: format!("eaid://ui_package/art/osd/components/hud/{key}.emb?context=default"),
        emb_edge: format!("eaid://ui_package/art/osd/components/hud/edge/{key}.emb?context=default"),
    }
}

/// One resource to add to a ready archive: name, type, fragments.
pub type Res = (String, &'static str, Vec<Vec<u8>>);

/// The surface, view and material of one sprite.
pub fn sprite_resources(n: &Names, art: &Art, fill_material: &[u8]) -> Result<Vec<Res>> {
    let (head, pixels) = surface(art)?;
    Ok(vec![
        (format!("{}.image", n.tex), "GfxSurfaceRes", vec![head, pixels]),
        (n.tex.clone(), "GfxSRVResource", vec![srv(&n.tex)]),
        (n.material.clone(), "GfxMaterialRes", vec![material(fill_material, &n.tex)?]),
    ])
}

/// The binding of the quad mesh's `slot` to the sprite's material.
pub fn binding_resource(n: &Names, slot: &str, edge: bool) -> Res {
    let name = if edge { &n.emb_edge } else { &n.emb };
    (name.clone(), "GfxMatBindRes", vec![emb(slot, &n.material)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> Vec<u8> {
        let m = Material {
            shader: "ui_texture".into(),
            params: vec![
                Param {
                    name: "me_colour".into(),
                    kind: CONST,
                    value: [0.5f32; 4].iter().flat_map(|f| f.to_le_bytes()).collect(),
                    text: String::new(),
                    ty: 0x40003,
                    class: 1,
                    cbuffer: 0,
                    offset: 0,
                    link: None,
                },
                Param {
                    name: "me_diffuseTexHeapIndex".into(),
                    kind: CONST,
                    value: vec![0; 4],
                    text: String::new(),
                    ty: HEAP.0,
                    class: HEAP.1,
                    cbuffer: 0,
                    offset: 16,
                    link: Some("eaid://old.tif".into()),
                },
                Param {
                    name: "me_diffuseTex".into(),
                    kind: TEXTURE,
                    value: Vec::new(),
                    text: "eaid://old.tif".into(),
                    ty: 0,
                    class: 0,
                    cbuffer: 0,
                    offset: 0,
                    link: None,
                },
            ],
            cbuffers: vec![("cb".into(), 32)],
        };
        write_material(&m)
    }

    #[test]
    fn a_material_round_trips_and_takes_the_texture_link() {
        let s = stock();
        assert_eq!(reencode_material(&s).unwrap(), s);
        let m = material(&s, "eaid://ui_package/art/osd/textures/hud/x.tif").unwrap();
        let p = parse_material(&m).unwrap();
        let tex = p.params.iter().find(|p| p.name == "me_diffuseTex").unwrap();
        assert_eq!(tex.text, "eaid://ui_package/art/osd/textures/hud/x.tif");
        let heap = p.params.iter().find(|p| p.name == "me_diffuseTexHeapIndex").unwrap();
        assert_eq!(heap.link.as_deref(), Some("eaid://ui_package/art/osd/textures/hud/x.tif"));
        let colour = &p.params[0].value;
        assert_eq!(&colour[..4], &1.0f32.to_le_bytes());
        assert!(material(&[0; 4], "x").is_err());
    }

    #[test]
    fn surfaces_are_bc7_srgb_with_a_40_byte_header() {
        let art = Art::solid(8, 4, [255, 255, 255, 255]);
        let (head, pix) = surface(&art).unwrap();
        assert_eq!(head.len(), 40);
        assert_eq!(u32::from_le_bytes(head[8..12].try_into().unwrap()), 70);
        assert_eq!(pix.len(), 8 * 4);
        assert!(Art::solid(6, 4, [0; 4]).check("x").is_err());
    }

    #[test]
    fn bindings_and_views_have_the_documented_layout() {
        let e = emb("central_bg_fill", "eaid://m.material");
        assert_eq!(&e[..12], &[1, 0, 0, 0, 1, 0, 0, 0, 24, 0, 0, 0]);
        assert_eq!(&e[20..36], b"central_bg_fill\0");
        let v = srv("eaid://t.tif");
        assert!(v.ends_with(b"eaid://t.tif.image\0"));
        assert_eq!(names("halo_", "base").tex, "eaid://ui_package/art/osd/textures/hud/halo_base.tif");
    }

    #[test]
    fn png_decode_expands_to_rgba() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 4, 4);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&[200u8; 48]).unwrap();
        }
        let art = decode_png(&out).unwrap();
        assert_eq!((art.width, art.height, art.rgba.len()), (4, 4, 64));
        assert_eq!(&art.rgba[..4], &[200, 200, 200, 255]);
        assert_eq!(hex_colour("#27F4D2").unwrap(), [0x27, 0xf4, 0xd2]);
    }
}
