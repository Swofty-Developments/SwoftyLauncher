//! The HUD editor's side: the preview `.uipre` setters, the template group that keeps the static
//! preview copy out of the game, and the selection box.
//!
//! The editor shows `dial_complete.idf` with no template running: the scene's own state plus its
//! `.uipre` preview setters. So each record gets a static copy of its elements (built from
//! `editor.preview` by [`super::scene::Builder::add_record`] in preview mode), the `.uipre` hides
//! the stock parts and fills the copy's texts, and in game a `<prefix>_pv_ctl` group hides the copy.
//! Setter keys and the element id rule (`djb2` of the name) are Project Halo's measurements of the
//! game's own `.uipre` files; Uses work from Project Halo by nyannoying
//! (https://github.com/nyannoying1337/f1-25-project-halo).

use std::collections::BTreeSet;

use swofty_formats::pkg::Node;
use swofty_formats::uipre::{self, Uipre};

use super::conflict::{Conflict, Kind, Setting, uipre_of};
use super::scene::Plan;
use crate::ui::idf;
use crate::ui::node::{self, NodeExt};
use crate::{Error, Result};

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(format!("hud: {msg}"))
}

struct Writer {
    p: Uipre,
    uipre: String,
}

impl Writer {
    fn open(stock: &[u8], scene: &str) -> Result<Self> {
        let p = Uipre::parse(stock)?;
        if p.target_id() != Some(idf::scene_id(scene)) {
            return Err(err(format!("the .uipre is not the preview data of {scene}")));
        }
        Ok(Self {
            p,
            uipre: uipre_of(scene),
        })
    }

    fn put(&mut self, path: &[&str], want: Setting) -> Result<()> {
        let key = want.key();
        match self.p.setter_at(path, key).map(|v| Setting::read(key, v)) {
            None => Ok(self.p.set_at(path, key, want.val())?),
            Some(have) if have == want => Ok(()),
            Some(have) => Err(Conflict::error(
                &self.uipre,
                Kind::Preview {
                    element: path.join(" > "),
                    have,
                    want,
                },
            )),
        }
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.p.to_bytes()?)
    }
}

fn distinct<'a>(names: impl IntoIterator<Item = &'a String>, within: Option<&str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(uipre::element_id(name)) {
            return Err(err(match within {
                Some(host) => format!("the preview element id of {name} is used twice in {host}"),
                None => format!("the preview element id of {name} is used twice"),
            }));
        }
    }
    Ok(())
}

/// The `.uipre` of the editor's scene with the stock `hide` parts hidden and each `(element,
/// text)` set. The setters are the shape of the stock ones (`Id` and `Setters` of `Key`, `Value`).
/// A setter the file already has with the same value (another addon's copy of the file hides the
/// same stock part) is kept as it is; one with another value is a [`Conflict`].
pub fn uipre(stock: &[u8], scene: &str, hide: &[String], texts: &[(String, String)]) -> Result<Vec<u8>> {
    let mut w = Writer::open(stock, scene)?;
    distinct(hide.iter().chain(texts.iter().map(|(e, _)| e)), None)?;
    for name in hide {
        w.put(&[name], Setting::Visible(false))?;
    }
    for (el, text) in texts {
        w.put(&[el], Setting::Text(text.clone()))?;
    }
    w.bytes()
}

/// One instance of another scene inside the editor's scene, with the elements inside it the editor
/// hides and shows and the labels it fills; `loc` labels take a game string id.
#[derive(Debug, Clone, Default)]
pub struct Instanced {
    pub host: String,
    pub hide: Vec<String>,
    pub show: Vec<String>,
    pub texts: Vec<(String, String)>,
    pub loc: Vec<(String, String)>,
}

/// [`uipre`] for a scene whose preview copy lives inside an instance of another scene (the standings
/// board's bottom cap): `hide` names the scene's own elements, each [`Instanced`] the elements inside
/// one instance, listed nested under the instance's id as the stock table lists its rows. An instance
/// the file already lists (another addon's own host) gets the setters added beside its own.
pub fn uipre_in(stock: &[u8], scene: &str, hide: &[String], instances: &[Instanced]) -> Result<Vec<u8>> {
    let mut w = Writer::open(stock, scene)?;
    distinct(hide.iter().chain(instances.iter().map(|i| &i.host)), None)?;
    for name in hide {
        w.put(&[name], Setting::Visible(false))?;
    }
    for i in instances {
        let names = i
            .hide
            .iter()
            .chain(&i.show)
            .chain(i.texts.iter().map(|(e, _)| e))
            .chain(i.loc.iter().map(|(e, _)| e));
        distinct(names, Some(&i.host))?;
        for name in &i.hide {
            w.put(&[&i.host, name], Setting::Visible(false))?;
        }
        for name in &i.show {
            w.put(&[&i.host, name], Setting::Visible(true))?;
        }
        for (el, text) in &i.texts {
            w.put(&[&i.host, el], Setting::Text(text.clone()))?;
        }
        for (el, id) in &i.loc {
            w.put(&[&i.host, el], Setting::Loc(id.clone()))?;
        }
    }
    w.bytes()
}

/// The group under a dial that hides the preview copy of a record in game. Stock parts it should
/// show again (the `.uipre` may apply in game too) get a static visible node each.
pub fn control(plan: &Plan, show_stock: &[String]) -> Node {
    let mut kids = vec![node::hidden(&format!("v_{}", plan.root), &[&plan.root])];
    for part in show_stock {
        kids.push(node::visible_node(&format!("v_{part}"), part, true, None, false));
    }
    node::group_node(&plan.ctl_name(), vec![], &[], kids)
}

/// Adds [`control`] under `parent`.
pub fn attach_control(template: &mut Node, parent: &str, plan: &Plan, show_stock: &[String]) -> Result<()> {
    let at = node::find_path(template, parent)?;
    let target = node::node_at_mut(template, &at)?;
    let name = plan.ctl_name();
    if target
        .slot(node::CHILDREN)?
        .iter()
        .any(|c| c.is_node() && c.name() == name)
    {
        return Err(err(format!("{parent} already has a {name}")));
    }
    let mut grp = control(plan, show_stock);
    node::fix_ids(&mut grp, Some(parent));
    target.slot_mut(node::CHILDREN)?.push(grp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use swofty_formats::pkg::{Arr, Obj, Pkg, Val};
    use swofty_formats::uipre::{KEY_TEXT, KEY_VISIBLE, element_id};

    use super::*;
    use crate::hud::{conflict, dials, model, screens};

    fn blank(scene: &str) -> Uipre {
        let bytes = Pkg {
            name: *b"UIPR",
            root: Node::Obj(Obj::new(vec![
                (Val::str("Elements"), Node::Arr(Arr::new(vec![]))),
                (
                    Val::str("TargetId"),
                    Node::Val(Val::raw(*b"ui64", idf::scene_id(scene).to_le_bytes())),
                ),
            ])),
            vca: vec![],
            layout: None,
        }
        .to_bytes()
        .unwrap();
        Uipre::parse(&bytes).unwrap()
    }

    fn halo_like() -> Vec<u8> {
        let mut p = blank(screens::STACKED);
        p.set_visible("mask", false).unwrap();
        p.set_visible_at(&["tt_host", screens::CAP_ANIM], false).unwrap();
        p.set_visible_at(&["tt_host", "tt_root"], false).unwrap();
        p.set_visible_at(&["tt_host", "tp_root"], true).unwrap();
        p.set_text_at(&["tt_host", "tp_r00_pos"], "10").unwrap();
        p.to_bytes().unwrap()
    }

    fn tower(pfx: &str) -> Instanced {
        Instanced {
            host: format!("{pfx}_host"),
            hide: vec![screens::CAP_ANIM.to_owned(), pfx.to_owned()],
            show: vec![format!("{pfx}_pv")],
            texts: vec![(format!("{pfx}_pv_r00_pos"), "10".to_owned())],
            loc: vec![(format!("{pfx}_pv_lapw"), "lng_lap".to_owned())],
        }
    }

    fn mask() -> Vec<String> {
        vec![screens::STACKED_MASK.to_owned()]
    }

    fn visible(p: &Uipre, path: &[&str]) -> Option<bool> {
        p.setter_at(path, KEY_VISIBLE)
            .map(|v| Setting::read(KEY_VISIBLE, v) == Setting::Visible(true))
    }

    #[test]
    fn a_single_addon_gets_the_setters_it_always_got() {
        let stock = blank(screens::STACKED).to_bytes().unwrap();
        let out = uipre_in(&stock, screens::STACKED, &mask(), &[tower("tvtt")]).unwrap();
        let mut want = blank(screens::STACKED);
        want.set_visible("mask", false).unwrap();
        want.set_visible_at(&["tvtt_host", screens::CAP_ANIM], false)
            .unwrap();
        want.set_visible_at(&["tvtt_host", "tvtt"], false).unwrap();
        want.set_visible_at(&["tvtt_host", "tvtt_pv"], true).unwrap();
        want.set_text_at(&["tvtt_host", "tvtt_pv_r00_pos"], "10").unwrap();
        want.set_loc_at(&["tvtt_host", "tvtt_pv_lapw"], "lng_lap")
            .unwrap();
        assert_eq!(out, want.to_bytes().unwrap());

        let dial = dials::facts(model::Dial::F125);
        let stock = blank(dial.scene).to_bytes().unwrap();
        let hide = vec!["frame".to_owned(), "fuel".to_owned()];
        let texts = vec![("one_hud_pv_speed".to_owned(), "212".to_owned())];
        let out = uipre(&stock, dial.scene, &hide, &texts).unwrap();
        let mut want = blank(dial.scene);
        want.set_visible("frame", false).unwrap();
        want.set_visible("fuel", false).unwrap();
        want.set_text("one_hud_pv_speed", "212").unwrap();
        assert_eq!(out, want.to_bytes().unwrap());
    }

    #[test]
    fn a_stock_part_another_addon_already_hides_stays_hidden_once() {
        let out = uipre_in(&halo_like(), screens::STACKED, &mask(), &[tower("tvtt")]).unwrap();
        let p = Uipre::parse(&out).unwrap();
        let mask_id = element_id("mask");
        assert_eq!(p.element_ids().iter().filter(|i| **i == mask_id).count(), 1);
        assert_eq!(p.setters(mask_id).len(), 1);
        assert_eq!(visible(&p, &["mask"]), Some(false));
        assert_eq!(visible(&p, &["tt_host", "tp_root"]), Some(true));
        assert_eq!(visible(&p, &["tt_host", "tt_root"]), Some(false));
        assert_eq!(visible(&p, &["tvtt_host", "tvtt_pv"]), Some(true));
        assert_eq!(visible(&p, &["tvtt_host", "tvtt"]), Some(false));
        assert_eq!(
            p.setter_at(&["tt_host", "tp_r00_pos"], KEY_TEXT)
                .and_then(Val::as_str),
            Some("10")
        );
        assert_eq!(
            p.setter_at(&["tvtt_host", "tvtt_pv_r00_pos"], KEY_TEXT)
                .and_then(Val::as_str),
            Some("10")
        );
    }

    #[test]
    fn the_same_setters_twice_change_nothing() {
        let stock = blank(screens::STACKED).to_bytes().unwrap();
        let once = uipre_in(&stock, screens::STACKED, &mask(), &[tower("tvtt")]).unwrap();
        let twice = uipre_in(&once, screens::STACKED, &mask(), &[tower("tvtt")]).unwrap();
        assert_eq!(once, twice);

        let dial = dials::facts(model::Dial::F125);
        let mut halo = blank(dial.scene);
        for kid in dials::STOCK_KIDS {
            halo.set_visible(kid, false).unwrap();
        }
        halo.set_text("halo_kmh", "217").unwrap();
        let hide: Vec<String> = dials::STOCK_KIDS.iter().map(|s| (*s).to_owned()).collect();
        let out = uipre(&halo.to_bytes().unwrap(), dial.scene, &hide, &[]).unwrap();
        assert_eq!(out, halo.to_bytes().unwrap());
    }

    #[test]
    fn a_setter_another_copy_sets_otherwise_is_a_conflict() {
        let mut p = blank(screens::STACKED);
        p.set_visible("mask", true).unwrap();
        let e = uipre_in(
            &p.to_bytes().unwrap(),
            screens::STACKED,
            &mask(),
            &[tower("tvtt")],
        )
        .unwrap_err();
        let e = conflict::by(e, &["pablo.tv-timing-tower".to_owned()]);
        let Error::HudConflict(c) = &e else { panic!("{e}") };
        assert_eq!(c.resource, screens::STACKED_UIPRE);
        assert_eq!(c.addons, ["pablo.tv-timing-tower"]);
        assert_eq!(
            c.kind,
            Kind::Preview {
                element: "mask".into(),
                have: Setting::Visible(true),
                want: Setting::Visible(false),
            }
        );
        assert_eq!(
            e.to_string(),
            "hud: another addon and pablo.tv-timing-tower set the HUD editor preview of the Track Position list differently: another addon shows mask, pablo.tv-timing-tower hides mask (driver_position_table_stacked.uipre from another addon)"
        );

        let mut p = blank(screens::STACKED);
        p.set_text_at(&["tvtt_host", "tvtt_pv_r00_pos"], "P1").unwrap();
        let e = uipre_in(
            &p.to_bytes().unwrap(),
            screens::STACKED,
            &mask(),
            &[tower("tvtt")],
        )
        .unwrap_err();
        let Error::HudConflict(c) = e else { panic!() };
        assert_eq!(
            c.kind,
            Kind::Preview {
                element: "tvtt_host > tvtt_pv_r00_pos".into(),
                have: Setting::Text("P1".into()),
                want: Setting::Text("10".into()),
            }
        );
    }

    #[test]
    fn two_composed_names_with_one_id_still_stop() {
        let stock = blank(screens::STACKED).to_bytes().unwrap();
        let host = Instanced {
            host: "mask".into(),
            ..Default::default()
        };
        let e = uipre_in(&stock, screens::STACKED, &mask(), &[host]).unwrap_err();
        assert!(!matches!(e, Error::HudConflict(_)), "{e}");
        assert!(e.to_string().contains("is used twice"), "{e}");
        let mut t = tower("tvtt");
        t.show.push("tvtt".into());
        let e = uipre_in(&stock, screens::STACKED, &mask(), &[t]).unwrap_err();
        assert!(e.to_string().contains("is used twice in tvtt_host"), "{e}");
        let other = blank(dials::facts(model::Dial::F125).scene).to_bytes().unwrap();
        assert!(uipre_in(&other, screens::STACKED, &mask(), &[]).is_err());
    }
}
