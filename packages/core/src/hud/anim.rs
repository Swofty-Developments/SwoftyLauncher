//! Anim clips and the scene's anim string table.
//!
//! TODO(stream F): `swofty_formats::animclip` replaces this writer once it lands. The packet fields
//! (`maxtime`, `mintime`, `interpolator`, `channels[0].misc = (0x80210 << 32) | (flags << 24) |
//! (type << 16) | ntimes`, `keyptr`, `timesptr`) are Project Halo's measurements of the game's
//! own clips; Uses work from Project Halo by nyannoying
//! (https://github.com/nyannoying1337/f1-25-project-halo).

use std::collections::BTreeMap;

use swofty_formats::pkg::{Node, Obj, Pkg, Val, Value};

use super::scene::Channel;
use crate::ui::idf;
use crate::ui::node::{NodeExt, ObjExt};
use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

fn set_int(o: &mut Obj, key: &str, v: u64) -> Result<()> {
    let val = o.at_mut(key)?.v_mut()?;
    let n = match &val.ty {
        b"ui64" | b"si64" => 8,
        b"ui16" => 2,
        b"ui08" | b"bool" => 1,
        _ => 4,
    };
    val.value = Value::Raw(v.to_le_bytes()[..n].to_vec());
    Ok(())
}

fn set_float(o: &mut Obj, key: &str, v: f64) -> Result<()> {
    o.at_mut(key)?.v_mut()?.value = Value::Raw((v as f32).to_le_bytes().to_vec());
    Ok(())
}

/// The strings of an `ACStringTable` with `names` appended, and each name's index. The table is
/// sorted and the new names must sort after every stock one, and among themselves.
pub fn append_names(strtab: &[u8], names: &[String]) -> Result<(Vec<u8>, BTreeMap<String, u32>)> {
    let mut p = Pkg::parse(strtab)?;
    let list = p.root.o_mut()?.at_mut("strings")?.as_arr_mut().ok_or_else(|| err("no strings"))?;
    let have: Vec<String> = list
        .items
        .iter()
        .filter_map(|v| v.as_val().and_then(Val::as_str).map(str::to_owned))
        .collect();
    let mut sorted = have.clone();
    sorted.sort();
    if have != sorted {
        return Err(err("the anim string table is not sorted"));
    }
    let mut ours = names.to_vec();
    ours.sort();
    ours.dedup();
    if let Some(last) = have.last()
        && let Some(low) = ours.iter().find(|n| *n <= last)
    {
        return Err(err(format!(
            "the anim node {low} sorts before the stock string {last}; anim nodes are named zh_..."
        )));
    }
    let ty = list.items.first().and_then(Node::as_val).map_or(*b"stri", |v| v.ty);
    let mut index = BTreeMap::new();
    for (i, n) in ours.iter().enumerate() {
        let mut v = Val::str(n.as_str());
        v.ty = ty;
        list.items.push(Node::Val(v));
        index.insert(n.clone(), (have.len() + i) as u32);
    }
    Ok((p.to_bytes()?, index))
}

/// An AnimClip (`temp`, `syst`) that animates `node` through `channels`, from a stock clip.
pub fn clip(
    template: &[u8],
    node: &str,
    name_index: u32,
    channels: &[Channel],
    max_time: f64,
    strtab: &str,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut p = Pkg::parse(template)?;
    let first = p
        .root
        .o()?
        .at("packets")?
        .a()?
        .first()
        .ok_or_else(|| err("the clip template has no packets"))?
        .clone();
    let mut syst: Vec<u8> = Vec::new();
    let align = |s: &mut Vec<u8>| s.resize(s.len().div_ceil(16) * 16, 0);
    let mut packets = Vec::new();
    for ch in channels {
        align(&mut syst);
        let tp = syst.len();
        ch.times.iter().for_each(|t| syst.extend_from_slice(&t.to_le_bytes()));
        align(&mut syst);
        let kp = syst.len();
        ch.keys.iter().flatten().for_each(|k| syst.extend_from_slice(&k.to_le_bytes()));
        let mut pk = idf::detached(&first);
        {
            let o = pk.o_mut()?;
            set_float(o, "maxtime", max_time)?;
            set_float(o, "mintime", 0.0)?;
            set_int(o, "interpolator", ch.interp as u64)?;
            let c = o
                .at_mut("channels")?
                .a_mut()?
                .first_mut()
                .ok_or_else(|| err("the clip template has no channel"))?;
            let mut c = idf::detached(c);
            {
                let co = c.o_mut()?;
                set_int(co, "name_index", name_index as u64)?;
                set_int(co, "mins", 0)?;
                let misc = (0x80210u64 << 32) | ((ch.flags as u64) << 24) | ((ch.typ as u64) << 16) | ch.times.len() as u64;
                set_int(co, "misc", misc)?;
                co.at_mut("name")?.v_mut()?.set_str(node);
                set_int(co, "keyptr", kp as u64)?;
                set_int(co, "timesptr", tp as u64)?;
            }
            *o.at_mut("channels")?.a_mut()? = vec![c];
        }
        packets.push(pk);
    }
    align(&mut syst);
    let root = p.root.o_mut()?;
    *root.at_mut("packets")?.a_mut()? = packets;
    root.at_mut("string_table")?.v_mut()?.set_str(strtab);
    Ok((p.to_bytes()?, syst))
}
