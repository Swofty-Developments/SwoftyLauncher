use std::path::{Path, PathBuf};

use swofty_formats::erp::{self, Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED, ZSTD, compress};
use swofty_formats::pkg::{Arr, Node, Obj, Pkg, Val};
use swofty_formats::texarray::Array;

use super::write::HANDLING_WHY;
use super::*;
use crate::Result;
use crate::asset::{AssetOp, FileKind, OpFile, Source};
use crate::content::runtime_assets::Runtime;
use crate::ops::ErpOp;
use crate::ui::node::component_id;

mod apply;
mod coherence;
mod overlay;
mod slices;
mod stats;
mod templates;
mod ui_check;
mod ui_merge;
mod write;

fn erp_ops(json: &str) -> Vec<ErpOp> {
    let file = OpFile::from_value(serde_json::from_str(json).unwrap(), FileKind::Erp).unwrap();
    file.ops
        .into_iter()
        .filter_map(|op| match op {
            AssetOp::Erp(op) => Some(op),
            _ => None,
        })
        .collect()
}

fn apply(b: &mut ErpBuild, ops: &[ErpOp], dir: &Path) -> Result<()> {
    ops.iter()
        .try_for_each(|op| b.apply(op, &|s: &Source| Ok(s.resolve(dir)?)))
}

fn layer(names: &[&str]) -> Vec<u8> {
    let items = names
        .iter()
        .map(|n| {
            Node::Obj(Obj::new(vec![
                (Val::str("name"), Node::Val(Val::str(*n))),
                (Val::str("id"), Node::Val(Val::u32(1))),
            ]))
        })
        .collect();
    Pkg {
        name: *b"INST",
        root: Node::Obj(Obj::new(vec![(
            Val::str("instances"),
            Node::Arr(Arr::new(items)),
        )])),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn erp(resources: &[(&str, Vec<u8>)]) -> Vec<u8> {
    typed_erp("WOInstances", resources)
}

fn typed_erp(kind: &str, resources: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&4u32.to_le_bytes());
    let resources = resources
        .iter()
        .map(|(name, body)| {
            let packed = compress(body).unwrap();
            let mut res = Resource {
                name: (*name).into(),
                kind: erp::kind(kind).unwrap(),
                unk: vec![0; 6],
                fragments: vec![Fragment::new(*b"temp", body.len() as u64, 0, ZSTD, packed)],
                hash: [0; 16],
            };
            res.hash = res.content_hash().unwrap();
            res
        })
        .collect();
    Erp {
        header,
        version: 4,
        resources,
    }
    .to_bytes()
    .unwrap()
}

fn spline(gates: &str) -> swofty_formats::bxml::Document {
    use swofty_formats::bxml::{Document, Element, Kind};
    let gate = |id: usize| Element {
        kind: Kind::Element,
        pad: 0,
        name: b"gate".to_vec(),
        attrs: vec![(b"name".to_vec(), format!("{gates}_{id:03}").into_bytes())],
        children: Vec::new(),
        end_pad: 0,
    };
    Document {
        root: Element {
            kind: Kind::Element,
            pad: 0,
            name: b"track".to_vec(),
            attrs: Vec::new(),
            children: (0..3).map(gate).collect(),
            end_pad: 0,
        },
        tail: Vec::new(),
    }
}

fn surface(name: &str, head: &[u8], pixels: &[u8]) -> Resource {
    let fragment = |compression: u8, data: &[u8]| {
        Fragment::new(
            *b"temp",
            data.len() as u64,
            0x10,
            compression,
            match compression {
                ZSTD => compress(data).unwrap(),
                _ => data.to_vec(),
            },
        )
    };
    let mut res = Resource {
        name: name.into(),
        kind: erp::kind("GfxSurfaceRes").unwrap(),
        unk: 9u32.to_le_bytes().to_vec(),
        fragments: vec![fragment(ZSTD, head), fragment(STORED, pixels)],
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    res
}

fn archive(resources: Vec<Resource>) -> Vec<u8> {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    Erp {
        header,
        version: 3,
        resources,
    }
    .to_bytes()
    .unwrap()
}

fn data(res: &Resource) -> Vec<Vec<u8>> {
    res.fragments.iter().map(|f| f.data().unwrap()).collect()
}

fn scene(uri: &str, tiles: &[&str]) -> Vec<u8> {
    let id = component_id(uri.strip_suffix(".idf").unwrap());
    let named = |n: &str| Node::Obj(Obj::new(vec![(Val::str("name"), Node::Val(Val::str(n)))]));
    let root = Obj::new(vec![
        (Val::str("name"), Node::Val(Val::str("Scene Root"))),
        (
            Val::str("children"),
            Node::Arr(Arr::new(tiles.iter().map(|t| named(t)).collect())),
        ),
    ]);
    let entry = Obj::new(vec![
        (Val::str("scene"), Node::Obj(root)),
        (Val::str("name"), Node::Val(Val::raw(*b"ui64", id.to_le_bytes()))),
    ]);
    Pkg {
        name: [0; 4],
        root: Node::Obj(Obj::new(vec![(
            Val::str("scenelist"),
            Node::Arr(Arr::new(vec![Node::Obj(entry)])),
        )])),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

const LOGOS: &str =
    "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.team_logos.f1_2026.texarray.image";

const UI: &str = "2025_asset_groups/ui_package/common_front_end.erp";

const BOOT: &str = "2025_asset_groups/vfx_package/common.erp";

fn array(width: u32, slices: &[[u8; 4]]) -> Resource {
    let mut head = vec![0u8; 40];
    head[12..16].copy_from_slice(&width.to_le_bytes());
    head[16..20].copy_from_slice(&2u32.to_le_bytes());
    head[28..32].copy_from_slice(&(slices.len() as u32).to_le_bytes());
    let fragment = |data: Vec<u8>| Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data);
    let mut r = Resource {
        name: LOGOS.into(),
        kind: erp::kind("GfxSurfaceRes").unwrap(),
        unk: vec![0; 4],
        fragments: vec![fragment(head), fragment(slices.concat())],
        hash: [0; 16],
    };
    r.hash = r.content_hash().unwrap();
    r
}

fn texture_erp(resources: Vec<Resource>) -> Vec<u8> {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    Erp {
        header,
        version: 3,
        resources,
    }
    .to_bytes()
    .unwrap()
}

fn slices_in(r: &Resource) -> Vec<u8> {
    r.fragments[1].data().unwrap().chunks(4).map(|c| c[0]).collect()
}

fn texture_game(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-erpbuild-slices-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    for (rel, body) in [
        (
            "2025_asset_groups/ui_package/a_other.erp",
            texture_erp(vec![Resource {
                name: "eaid://ui_package/x.image".into(),
                ..array(2, &[[0; 4], [0; 4]])
            }]),
        ),
        (UI, texture_erp(vec![array(2, &[[0; 4], [1; 4], [2; 4], [3; 4]])])),
        (
            BOOT,
            texture_erp(vec![Resource {
                name: "eaid://vfx_package/y.image".into(),
                ..array(2, &[[0; 4], [0; 4]])
            }]),
        ),
    ] {
        std::fs::create_dir_all(game.join(rel).parent().unwrap()).unwrap();
        std::fs::write(game.join(rel), body).unwrap();
    }
    root
}

fn merge_op(erp: &str, source: &str) -> ErpOp {
    ErpOp::MergeSlices {
        erp: erp.into(),
        resource: LOGOS.into(),
        source: Source::addon(source),
    }
}

fn view_res(count: u32) -> Resource {
    let mut v = vec![0u8; 128];
    v[0x14..0x18].copy_from_slice(&count.to_le_bytes());
    let mut r = Resource {
        name: LOGOS.strip_suffix(".image").unwrap().into(),
        kind: erp::kind("GfxSurfaceRes").unwrap(),
        unk: vec![0; 4],
        fragments: vec![Fragment::new(*b"temp", v.len() as u64, 0x10, STORED, v)],
        hash: [0; 16],
    };
    r.hash = r.content_hash().unwrap();
    r
}

fn view_count(r: &Resource) -> u32 {
    u32::from_le_bytes(r.fragments[0].data().unwrap()[0x14..0x18].try_into().unwrap())
}
