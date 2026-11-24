//! Composing the records of every addon into the overlay archives, one scene growth, template pass and string
//! table per dial.

use super::conflict;
use super::dials::DialFacts;
use super::inputs::prefix_for;
use super::model::{Dial, StockMode};
use super::scene::{Builder, Mode, Plan, Sprites};
use super::sprites::{Expanded, Loaded, SpriteEnv, load_sprites, stock_parts, tinted_sprites};
use super::stock::Stock;
use super::stock_edit::{self, StockNodes};
use super::template::StockTags;
use super::textures::Res;
use super::{Built, Input, Output, TemplateBase, err};
use super::{dials, editor, scene, screen_build, screens, template, textures, units};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use swofty_formats::animclip;
use swofty_formats::erp::{Erp, Packer, Resource};
use swofty_formats::pkg::Pkg;

pub(super) fn first(v: Vec<Vec<u8>>, what: &str) -> Result<Vec<u8>> {
    v.into_iter()
        .next()
        .ok_or_else(|| err(format!("{what} has no data")))
}

/// The game's own race and spectator templates, read the first time a decision looks up its stock
/// MVCD: the copies the composer starts from carry other addons' nodes.
struct GameTemplates<'a> {
    stock: &'a dyn Stock,
    race: std::cell::OnceCell<Pkg>,
    spectator: std::cell::OnceCell<Pkg>,
}

impl<'a> GameTemplates<'a> {
    fn new(stock: &'a dyn Stock) -> Self {
        Self {
            stock,
            race: std::cell::OnceCell::new(),
            spectator: std::cell::OnceCell::new(),
        }
    }

    fn read(&self, name: &str) -> Result<Pkg> {
        Ok(Pkg::parse(&first(
            self.stock.game_copy(dials::STOCK_ERP, name)?,
            name,
        )?)?)
    }

    fn race(&self) -> Result<&Pkg> {
        if self.race.get().is_none() {
            let _ = self.race.set(self.read(dials::TEMPLATE)?);
        }
        Ok(self.race.get().expect("set above"))
    }

    fn spectator(&self) -> Result<&Pkg> {
        if self.spectator.get().is_none() {
            let _ = self.spectator.set(self.read(dials::SPECTATOR)?);
        }
        Ok(self.spectator.get().expect("set above"))
    }
}

/// Composes `inputs` against the stock files into the overlay archives.
pub fn compose(stock: &dyn Stock, inputs: &[Input]) -> Result<Output> {
    compose_with(stock, inputs, None, &Packer::default())
}

/// [`compose`] with `base` applied to the template the composer starts from. The composer is the last
/// step: its nodes go on top of `base`'s edit and of whatever `stock` reads.
pub fn compose_with(
    stock: &dyn Stock,
    inputs: &[Input],
    base: Option<TemplateBase>,
    packer: &Packer,
) -> Result<Output> {
    let mut out = Output::default();
    if inputs.is_empty() {
        return Ok(out);
    }
    let mut prefixes = BTreeMap::new();
    for (i, input) in inputs.iter().enumerate() {
        let p = prefix_for(&input.addon, &input.key, input.record.prefix.as_deref())?;
        if let Some(j) = prefixes.insert(p.clone(), i) {
            return Err(err(format!(
                "{} and {} both get the prefix {p}; set `prefix` in one of them",
                inputs[j].addon, input.addon
            )));
        }
    }
    let pfx_of: Vec<String> = {
        let mut v = vec![String::new(); inputs.len()];
        for (p, i) in &prefixes {
            v[*i] = p.clone();
        }
        v
    };

    let mut is_screen = Vec::with_capacity(inputs.len());
    for input in inputs {
        let t = screens::targets(&input.record)
            .map_err(|e| err(format!("{}/{}: {e}", input.addon, input.key)))?;
        is_screen.push(t.is_some());
    }
    let yields: Vec<(String, &Input)> = inputs
        .iter()
        .zip(&pfx_of)
        .filter(|(i, _)| i.record.yields.is_some())
        .map(|(i, p)| (p.clone(), i))
        .collect();
    let mut live: Vec<Expanded> = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        if input.record.scene_target().is_some() || is_screen[i] || input.record.yields.is_some() {
            continue;
        }
        live.push(Expanded {
            input: i,
            rec: input.record.clone(),
            switch: input.record.switch.clone(),
        });
        for (name, v) in &input.record.variants {
            live.push(Expanded {
                input: i,
                rec: input.record.with_variant(name)?,
                switch: v.switch.clone().or_else(|| input.record.switch.clone()),
            });
        }
    }

    let read = |name: &str| first(stock.read(dials::STOCK_ERP, name)?, name);
    let fill = read(dials::FILL_MATERIAL)?;
    let mods = read(dials::MODS)?;
    let bar = read(dials::BAR)?;
    let clip_template = read(dials::CLIP_TEMPLATE)?;
    let om_xm = read(dials::F126.scene)?;
    let stacked = read(scene::STACKED)?;
    let lap_counter = read(scene::LAP_COUNTER)?;

    let used: std::collections::BTreeSet<Dial> = live.iter().map(|e| e.rec.dial).collect();
    let mut resources: BTreeMap<String, Res> = BTreeMap::new();
    let mut loaded: Vec<Loaded> = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        if is_screen[i] || input.record.yields.is_some() {
            loaded.push(Loaded {
                sprites: Sprites::new(),
                resources: Vec::new(),
            });
            continue;
        }
        let mut tinted = std::collections::BTreeSet::new();
        tinted_sprites(&input.record.elements, &mut tinted);
        let team = match input
            .record
            .editor
            .as_ref()
            .and_then(|e| e.team_colour.as_deref())
        {
            Some(c) => textures::hex_colour(c)?,
            None => [0x27, 0xf4, 0xd2],
        };
        let env = SpriteEnv {
            stock,
            fill_material: &fill,
            uses_fill: true,
            uses_edge: used.contains(&Dial::F2),
        };
        let l = load_sprites(&env, input, &pfx_of[i], &tinted, team)?;
        for r in &l.resources {
            resources.insert(r.0.clone(), r.clone());
        }
        loaded.push(l);
    }

    let template_bytes = read(dials::TEMPLATE)?;
    let spectator_bytes = read(dials::SPECTATOR)?;
    let race_stock = Pkg::parse(&template_bytes)?;
    let spec_stock = Pkg::parse(&spectator_bytes)?;
    let mut template = match base {
        Some(edit) => Pkg::parse(&edit(&template_bytes)?)?,
        None => race_stock.clone(),
    };
    let mut spectator = spec_stock.clone();
    let race_tags = StockTags::race(&race_stock.root);
    let game_templates = GameTemplates::new(stock);

    let mut spectator_used = false;
    let mut preview_hides: Vec<String> = Vec::new();
    let mut pv_texts: Vec<(String, String)> = Vec::new();
    let mut scene_resources: Vec<Res> = Vec::new();
    let mut preview_plans: Vec<(usize, Plan)> = Vec::new();
    let mut live_f125_hides = false;

    for dial in Dial::ALL {
        let facts: &'static DialFacts = dials::facts(dial);
        let here: Vec<&Expanded> = live.iter().filter(|e| e.rec.dial == dial).collect();
        let want_preview = dial == Dial::F125 && !live.is_empty();
        if here.is_empty() && !want_preview {
            continue;
        }
        let scene_bytes = first(stock.read(dials::STOCK_ERP, facts.scene)?, facts.scene)?;
        let text_from = (dial != Dial::F126).then_some(om_xm.as_slice());
        let mut b = Builder::new(facts, &scene_bytes, &mods, &bar, text_from)?;
        b.with_donors(&stacked, &lap_counter)?;
        let end = anim_end(stock, facts);
        if here.iter().any(|e| units::used(&e.rec.elements)) {
            b.units = units::load(stock, dials::STOCK_ERP)?;
        }
        let mut parked = false;
        let mut footprint: Option<[f64; 4]> = None;
        let mut plans: Vec<(&Expanded, Plan)> = Vec::new();
        for e in &here {
            let parts = stock_parts(&e.rec);
            b.stock_parts_present(&parts)?;
            let mode = e.rec.effective_stock();
            if mode == StockMode::Park && !parked {
                b.park(&parts)?;
                parked = true;
            }
            b.anim = anim_of(
                end.as_deref(),
                facts,
                &pfx_of[e.input],
                &inputs[e.input].addon,
                &mut out.lines,
            )?;
            let mut plan = b.add_record(&e.rec, &pfx_of[e.input], &loaded[e.input].sprites, Mode::Live)?;
            let sw = e
                .switch
                .as_ref()
                .map(|s| template::resolve(s, template::Usage::Visible, "variant switch"))
                .transpose()?;
            let on = e
                .switch
                .as_ref()
                .and_then(|b| b.default.as_ref())
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            if plan.root_hidden || sw.is_some() {
                plan.vis.insert(
                    0,
                    scene::VisItem {
                        name: format!("v_{}", plan.root),
                        element: plan.root.clone(),
                        db: sw.as_ref().map(|s| s.db.clone()),
                        negate: sw.as_ref().is_some_and(|s| s.negate),
                        default: on,
                    },
                );
            }
            if mode == StockMode::Hide {
                for part in &parts {
                    plan.vis.push(scene::VisItem {
                        name: format!("v_stock_{part}"),
                        element: part.clone(),
                        db: sw.as_ref().map(|s| s.db.clone()),
                        negate: sw.as_ref().is_some_and(|s| !s.negate),
                        default: !on,
                    });
                }
                if dial == Dial::F125 {
                    live_f125_hides = true;
                }
            }
            if mode == StockMode::Park && dial == Dial::F125 {
                live_f125_hides = true;
            }
            let sb = scene::scene_box(&plan);
            footprint = Some(footprint.map_or(sb, |f| scene::union_box(f, sb)));
            plans.push((e, plan));
        }
        if want_preview {
            for (i, input) in inputs.iter().enumerate() {
                let rec = &input.record;
                if rec.scene_target().is_some() || is_screen[i] || rec.yields.is_some() {
                    continue;
                }
                let state = rec.editor.as_ref().map(|e| e.preview.clone()).unwrap_or_default();
                let plan = b.add_record(rec, &pfx_of[i], &loaded[i].sprites, Mode::Preview(&state))?;
                let sb = match rec.editor.as_ref().and_then(|e| e.rect) {
                    Some(r) => {
                        let mut p = plan.clone();
                        p.footprint = r;
                        scene::scene_box(&p)
                    }
                    None => scene::scene_box(&plan),
                };
                footprint = Some(footprint.map_or(sb, |f| scene::union_box(f, sb)));
                if rec
                    .editor
                    .as_ref()
                    .and_then(|e| e.hide_stock_in_editor)
                    .unwrap_or(true)
                {
                    for p in stock_parts(rec) {
                        if !preview_hides.contains(&p) {
                            preview_hides.push(p);
                        }
                    }
                }
                pv_texts.extend(plan.pv_texts.clone());
                preview_plans.push((i, plan));
            }
        }
        let mut new_scene = b.finish(footprint)?;
        let mut taken = BTreeSet::new();
        let mut by_pfx: Vec<usize> = (0..plans.len()).collect();
        by_pfx.sort_by(|a, b| pfx_of[plans[*a].0.input].cmp(&pfx_of[plans[*b].0.input]));
        for i in by_pfx {
            let (e, plan) = &mut plans[i];
            if e.rec.effective_stock() != StockMode::Hide {
                continue;
            }
            let pfx = &pfx_of[e.input];
            let entry = StockNodes {
                template: facts.race_parent.to_owned(),
                scene: None,
                hide: stock_parts(&e.rec),
                wrap: Vec::new(),
                while_: None,
            };
            let (entry, moved) = stock_edit::share(&entry, facts.scene, &mut taken);
            if moved.is_empty() {
                continue;
            }
            let (bytes, names) = stock_edit::wrap_scene(&new_scene, &entry, pfx, None)?;
            new_scene = bytes;
            for (part, wrapper) in moved.iter().zip(&names) {
                let name = format!("v_stock_{part}");
                for v in plan.vis.iter_mut().filter(|v| v.name == name) {
                    v.element = wrapper.clone();
                }
            }
            out.lines.push(format!(
                "{pfx}: another record hides the stock {} of {} too, so this record hides them through wrapper nulls",
                moved.join(", "),
                facts.scene.rsplit('/').next().unwrap_or(facts.scene)
            ));
        }
        let planned: Vec<&Plan> = plans.iter().map(|(_, p)| p).collect();
        clip_resources(stock, facts, &clip_template, &planned, &mut scene_resources)?;
        scene_resources.push((
            facts.scene.to_owned(),
            "UILayout",
            vec![std::mem::take(&mut new_scene)],
        ));
        for (e, plan) in &plans {
            let mvcd = plan.decisions.iter().any(|d| d.kind == "mvcd");
            template::attach(
                &mut template.root,
                &race_stock.root,
                match mvcd {
                    true => &game_templates.race()?.root,
                    false => &race_stock.root,
                },
                &race_tags,
                facts.race_parent,
                plan,
            )?;
            if e.rec.spectator {
                let tags = StockTags::spectator(
                    &spec_stock.root,
                    facts.spectator_parent,
                    &race_tags,
                    facts.race_parent,
                    &race_stock.root,
                )?;
                template::attach(
                    &mut spectator.root,
                    &spec_stock.root,
                    match mvcd {
                        true => &game_templates.spectator()?.root,
                        false => &spec_stock.root,
                    },
                    &tags,
                    facts.spectator_parent,
                    plan,
                )?;
                spectator_used = true;
            }
            out.lines.push(format!(
                "{}: {} in {}: {} template nodes, {} clips",
                pfx_of[e.input],
                facts.dial.key(),
                facts.scene.rsplit('/').next().unwrap_or(facts.scene),
                plan.vis.len()
                    + plan.texts.len()
                    + plan.fractions.len()
                    + plan.colours.len()
                    + plan.shows.len(),
                plan.clips.len()
            ));
        }
    }

    let f125 = dials::facts(Dial::F125);
    let keep_stock: Vec<String> = if live_f125_hides {
        Vec::new()
    } else {
        dials::STOCK_KIDS.iter().map(|s| (*s).to_owned()).collect()
    };
    for (i, plan) in &preview_plans {
        editor::attach_control(&mut template.root, f125.race_parent, plan, &keep_stock)?;
        if inputs[*i].record.spectator {
            editor::attach_control(&mut spectator.root, f125.spectator_parent, plan, &keep_stock)?;
            spectator_used = true;
        }
    }
    if !preview_plans.is_empty() {
        let stock_uipre = first(stock.read(dials::STOCK_ERP, f125.uipre)?, f125.uipre)?;
        let addons = conflict::addons_of(preview_plans.iter().map(|(i, _)| inputs[*i].addon.as_str()));
        let new = editor::uipre(&stock_uipre, f125.scene, &preview_hides, &pv_texts)
            .map_err(|e| conflict::by(e, &addons))?;
        scene_resources.push((f125.uipre.to_owned(), "UIPR", vec![new]));
    }
    let mut targets: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, input) in inputs.iter().enumerate() {
        if let Some((scene, parent)) = input.record.scene_target() {
            targets
                .entry((scene.to_owned(), parent.to_owned()))
                .or_default()
                .push(i);
        }
    }
    let mut target_scenes: Vec<Res> = Vec::new();
    for ((scene, parent), members) in &targets {
        let facts = dials::target_facts(scene, parent);
        let bytes = first(stock.read(dials::STOCK_ERP, scene)?, scene)?;
        let mut b = Builder::for_scene(facts, &bytes, &om_xm, &mods, &bar)?;
        b.with_donors(&stacked, &lap_counter)?;
        let end = anim_end(stock, facts);
        let mut plans = Vec::new();
        for &i in members {
            b.anim = anim_of(
                end.as_deref(),
                facts,
                &pfx_of[i],
                &inputs[i].addon,
                &mut out.lines,
            )?;
            let plan = b.add_scene_record(&inputs[i].record, &pfx_of[i], &loaded[i].sprites)?;
            plans.push((i, plan));
        }
        target_scenes.push((scene.clone(), "UILayout", vec![b.finish(None)?]));
        let planned: Vec<&Plan> = plans.iter().map(|(_, p)| p).collect();
        clip_resources(stock, facts, &clip_template, &planned, &mut target_scenes)?;
        for (i, plan) in &plans {
            let checked = template::attach_scene(&mut template.root, &race_tags, parent, scene, plan)?;
            out.lines.push(format!(
                "{}: {} in {}: {} template nodes, {checked} bindings checked against the stock template",
                pfx_of[*i],
                parent,
                scene.rsplit('/').next().unwrap_or(scene),
                plan.vis.len() + plan.texts.len() + plan.shows.len(),
            ));
        }
    }
    let screens_out = screen_build::compose_screens(
        stock,
        inputs,
        &pfx_of,
        &mut template,
        &race_stock,
        &mut spectator,
        &spec_stock,
        &fill,
        &mods,
        &bar,
        &om_xm,
    )?;
    spectator_used |= screens_out.spectator_used;
    out.lines.extend(screens_out.lines.iter().cloned());
    out.editor = screens_out.editor.clone();
    let mut all: Vec<Res> = resources.into_values().collect();
    all.extend(scene_resources);
    if !all.is_empty() {
        out.archives.push(Built {
            file: "hud_scene.erp".into(),
            erps: dials::SCENE_ERPS.iter().map(|s| (*s).to_owned()).collect(),
            bytes: archive(stock, all, packer)?,
        });
    }
    if !target_scenes.is_empty() {
        let erps: Vec<String> = dials::TEMPLATE_ERPS
            .iter()
            .filter(|erp| targets.keys().all(|(scene, _)| stock.read(erp, scene).is_ok()))
            .map(|e| (*e).to_owned())
            .collect();
        if erps.is_empty() {
            return Err(err(
                "no archive that has the template also has every scene a record targets",
            ));
        }
        out.archives.push(Built {
            file: "hud_target_scene.erp".into(),
            erps,
            bytes: archive(stock, target_scenes, packer)?,
        });
    }
    out.archives.push(Built {
        file: "hud_template.erp".into(),
        erps: dials::TEMPLATE_ERPS.iter().map(|s| (*s).to_owned()).collect(),
        bytes: archive(
            stock,
            vec![(dials::TEMPLATE.to_owned(), "UIDF", vec![template.to_bytes()?])],
            packer,
        )?,
    });
    if spectator_used {
        let mut res = vec![(dials::SPECTATOR.to_owned(), "UIDF", vec![spectator.to_bytes()?])];
        res.extend(screens_out.spectator_scene.iter().cloned());
        out.archives.push(Built {
            file: "hud_spectator.erp".into(),
            erps: dials::SPECTATOR_ERPS.iter().map(|s| (*s).to_owned()).collect(),
            bytes: archive(stock, res, packer)?,
        });
    }
    out.archives
        .extend(screen_build::archives(stock, &screens_out, packer)?);
    super::yields::apply(stock, &yields, base, &mut out.archives, &mut out.lines, packer)?;
    cover_carriers(stock, &mut out.archives, &mut out.lines)?;
    Ok(out)
}

/// Overlays every stock resource of `archives` on every stock archive that carries it, not only on
/// the archives each is made for: the game loads a resource from whichever archive reads it first,
/// so a stock copy left beside a composed one mixes the two. Each added archive holds the stock
/// resources its targets carry and every new resource of the archive it comes from, so a composed
/// copy keeps the clips, sprites and materials it references.
pub fn cover_carriers(stock: &dyn Stock, archives: &mut Vec<Built>, lines: &mut Vec<String>) -> Result<()> {
    let parsed: Vec<Erp> = archives
        .iter()
        .map(|a| Erp::parse(&a.bytes).map_err(|e| err(format!("{}: {e}", a.file))))
        .collect::<Result<_>>()?;
    let names: Vec<&str> = parsed
        .iter()
        .flat_map(|e| e.resources.iter().map(|r| r.name.as_str()))
        .collect();
    let carriers = stock.carriers(&names)?;
    let mut lands: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (a, e) in archives.iter().zip(&parsed) {
        for r in &e.resources {
            lands
                .entry(r.name.clone())
                .or_default()
                .extend(a.erps.iter().cloned());
        }
    }
    let mut added = Vec::new();
    for (a, e) in archives.iter().zip(&parsed) {
        let mut held: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut new = BTreeSet::new();
        for (i, r) in e.resources.iter().enumerate() {
            let of = carriers.get(&r.name).map(Vec::as_slice).unwrap_or_default();
            if of.is_empty() {
                new.insert(i);
            }
            for erp in of.iter().filter(|erp| !lands[&r.name].contains(*erp)) {
                held.entry(erp.clone()).or_default().insert(i);
            }
        }
        let mut groups: BTreeMap<BTreeSet<usize>, Vec<String>> = BTreeMap::new();
        for (erp, idx) in held {
            groups.entry(idx).or_default().push(erp);
        }
        let stem = a.file.strip_suffix(".erp").unwrap_or(&a.file);
        for (n, (idx, erps)) in groups.into_iter().enumerate() {
            for i in &idx {
                if let Some(l) = lands.get_mut(&e.resources[*i].name) {
                    l.extend(erps.iter().cloned());
                }
            }
            let resources: Vec<Resource> = e
                .resources
                .iter()
                .enumerate()
                .filter(|(i, _)| idx.contains(i) || new.contains(i))
                .map(|(_, r)| r.clone())
                .collect();
            let file = format!("{stem}_{}.erp", n + 2);
            lines.push(format!(
                "{file}: {} also carry {} of the stock resources {} writes ({}), so they get the composed copies and the {} new resources beside them",
                erps.iter().map(|e| e.rsplit('/').next().unwrap_or(e)).collect::<Vec<_>>().join(", "),
                idx.len(),
                a.file,
                idx.iter()
                    .map(|i| e.resources[*i].name.rsplit('/').next().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(", "),
                new.len()
            ));
            let mut erp = Erp {
                header: e.header,
                version: e.version,
                resources,
            };
            added.push(Built {
                file,
                erps,
                bytes: erp.to_bytes()?,
            });
        }
    }
    archives.extend(added);
    Ok(())
}

/// The last name of the anim string table of the scene of `facts`, when the table is sorted: the anim
/// names a record adds to the scene sort after it ([`clip_resources`]).
pub(super) fn anim_end(stock: &dyn Stock, facts: &DialFacts) -> Option<String> {
    let strtab = first(stock.read(dials::STOCK_ERP, facts.strtab).ok()?, facts.strtab).ok()?;
    let have = animclip::read_strings(&strtab).ok()?;
    match have.is_sorted() {
        true => have.last().cloned(),
        false => None,
    }
}

/// The anim prefix of the record `pfx` of `addon` in the scene of `facts`, whose anim string table ends
/// at `end` ([`scene::anim_prefix`]), or the refusal naming the addon whose copy of the table leaves
/// the record no name that sorts after it.
pub(super) fn anim_of(
    end: Option<&str>,
    facts: &DialFacts,
    pfx: &str,
    addon: &str,
    lines: &mut Vec<String>,
) -> Result<String> {
    let last = end.unwrap_or_default();
    let Some(anim) = scene::anim_prefix(end, pfx) else {
        let kind = conflict::Kind::Clips {
            scene: facts.scene.to_owned(),
            last: last.to_owned(),
        };
        return Err(conflict::by(
            conflict::Conflict::error(facts.strtab, kind),
            &[addon.to_owned()],
        ));
    };
    if anim != scene::ANIM_PREFIX {
        lines.push(format!(
            "{pfx}: the anim string table of {} already ends at {last}, so this record's anim names start {anim}",
            facts.scene.rsplit('/').next().unwrap_or(facts.scene)
        ));
    }
    Ok(anim)
}

/// The anim string table and the clips the plans put in `facts.scene`: the new names appended
/// to the stock table in order, one `AnimClip` per clip, built on `clip_template`. Nothing when
/// no plan has a clip.
pub fn clip_resources(
    stock: &dyn Stock,
    facts: &DialFacts,
    clip_template: &[u8],
    plans: &[&Plan],
    out: &mut Vec<Res>,
) -> Result<()> {
    let mut names: Vec<String> = plans.iter().flat_map(|p| p.anim_names()).collect();
    names.sort();
    if names.is_empty() {
        return Ok(());
    }
    let strtab = first(stock.read(dials::STOCK_ERP, facts.strtab)?, facts.strtab)?;
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let (tab, first_index) = animclip::append_strings(&strtab, &refs)?;
    out.push((facts.strtab.to_owned(), "ACStringTable", vec![tab]));
    for plan in plans {
        for c in &plan.clips {
            let at = names.iter().position(|n| *n == c.node).expect("collected above");
            let (temp, syst) = animclip::build_clip(
                clip_template,
                &c.node,
                (first_index + at) as i32,
                facts.strtab,
                &c.channels,
            )?;
            out.push((
                format!("{}/{}?context=animation", facts.scene, c.node),
                "AnimClip",
                vec![temp, syst],
            ));
        }
    }
    Ok(())
}

/// A ready archive of `resources`, each laid out like the stock resource it is made after.
pub fn archive(stock: &dyn Stock, resources: Vec<Res>, packer: &Packer) -> Result<Vec<u8>> {
    let (header, version) = stock.header()?;
    let mut out = Vec::with_capacity(resources.len());
    let mut seen = std::collections::BTreeSet::new();
    for (name, kind, frags) in resources {
        if !seen.insert(name.clone()) {
            return Err(err(format!("the resource {name} is made twice")));
        }
        let template = stock.layout(&name, kind, frags.len())?;
        out.push(Resource::like_with(packer, &name, &template, &frags)?);
    }
    let mut erp = Erp {
        header,
        version,
        resources: out,
    };
    Ok(erp.to_bytes()?)
}
