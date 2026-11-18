mod assets;
mod db;
mod overlay;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_formats::erp::{Erp, Resource};
use swofty_formats::lng::Lng;

use crate::asset::{AssetOp, FileOp, GroupOp, Kind, LangOp, Op, Source};
use crate::manifest::relative_inside;
use crate::{Error, Result, io_err};

use overlay::parts_of;

pub use assets::{
    Assets, DbEdit, Group, LangTable, PostFxLive, PostFxPass, PostFxTexture, PostFxToggle, PostFxWhen,
    ResErp, ShaderLook,
};
pub use db::{
    RUNTIME_RECORD_MARGIN, STOCK_LOADED_RECORDS, db_edits, runtime_loaded_estimate, runtime_record_room,
    runtime_text,
};
pub use overlay::overlay_fits;
pub(crate) use overlay::{foreign_argument, layout_change};

pub const ASSETS_FILE: &str = "swofty/assets.json";
pub(crate) const SWOFTY: &str = "swofty";
const RESOURCES_DIR: &str = "assets/resources";
const LANG_DIR: &str = "assets/lang";
const OVERLAY_DIR: &str = "overlay";
/// The active shader look's packed LUT, under `swofty/`.
pub const SHADER_LUT: &str = "shaders/look.lut";
/// The compiled post-process passes, under `swofty/`.
pub const POSTFX_DIR: &str = "postfx";

// A ready overlay archive from an erp.overlay op, loaded as it is.
#[derive(Debug, Clone)]
pub struct Prebuilt {
    pub origin: String,
    pub path: PathBuf,
    pub erps: Vec<String>,
    pub resources: usize,
}

#[derive(Debug, Default)]
pub struct Runtime {
    pub assets: Assets,
    // An addon turned on an experimental feature of the core, which needs the
    // file even with no assets.
    enabled: bool,
    resources: usize,
    pub files: BTreeMap<String, PathBuf>,
    tables: BTreeMap<String, Lng>,
    defaults: BTreeMap<String, BTreeMap<String, String>>,
    look: Option<Vec<u8>>,
    passes: Vec<Vec<u8>>,
    images: Vec<Vec<u8>>,
}

fn lang_file(language: &str) -> String {
    match language {
        "*" => format!("{LANG_DIR}/all.lng"),
        code => format!("{LANG_DIR}/{code}.lng"),
    }
}

impl Runtime {
    pub fn new(features: Vec<Kind>) -> Self {
        Self {
            assets: Assets {
                features: features.iter().map(Kind::to_string).collect(),
                ..Assets::default()
            },
            ..Self::default()
        }
    }

    /// Turns on an experimental feature of the core, such as `overlay`.
    pub fn enable(&mut self, feature: &str) {
        if !self.assets.features.iter().any(|f| f == feature) {
            self.assets.features.push(feature.to_owned());
        }
        self.enabled = true;
    }

    /// Grades the picture with a packed LUT at `strength`, from 0 to 1.
    pub fn shader(&mut self, packed: Vec<u8>, strength: f64, addon: &str) {
        self.assets.shader = Some(ShaderLook {
            lut: SHADER_LUT.into(),
            strength,
            addon: addon.into(),
        });
        self.look = Some(packed);
    }

    /// Adds a compiled post-process pass after the ones added before it.
    pub fn postfx(&mut self, dxil: Vec<u8>, mut pass: PostFxPass) {
        pass.shader = format!("{POSTFX_DIR}/{:02}.dxil", self.passes.len());
        self.assets.postfx.push(pass);
        self.passes.push(dxil);
    }

    /// Adds a texture the passes read or write; `image` is the `.tex` of an image texture.
    pub fn postfx_texture(&mut self, mut texture: PostFxTexture, image: Option<Vec<u8>>) {
        if let Some(tex) = image {
            texture.file = Some(format!("{POSTFX_DIR}/tex/{:02}.tex", self.images.len()));
            self.images.push(tex);
        }
        self.assets.postfx_textures.push(texture);
    }

    pub fn postfx_toggle(&mut self, addon_id: &str, option: &str) {
        self.assets.postfx_toggles.push(PostFxToggle {
            addon_id: addon_id.into(),
            option: option.into(),
        });
    }

    fn place(&mut self, rel: String, src: PathBuf) -> Result<()> {
        let rel = format!("{SWOFTY}/{rel}");
        if self.files.insert(rel.clone(), src).is_some() {
            return Err(Error::Other(format!("{rel} is placed twice")));
        }
        Ok(())
    }

    pub fn push(&mut self, op: Op, resolve: &dyn Fn(&Op, &Source) -> Result<PathBuf>) -> Result<()> {
        match &op.op {
            AssetOp::Group(GroupOp::Register {
                group,
                erp,
                erps,
                deps,
                template,
            }) => self.assets.asset_groups.push(Group {
                group: group.clone(),
                erps: erp.iter().chain(erps).cloned().collect(),
                deps: deps.clone(),
                template: template.clone(),
                origin: op.origin.to_string(),
            }),
            AssetOp::Lang(LangOp::Set { language, key, value }) => {
                self.tables
                    .entry(language.clone())
                    .or_default()
                    .set(key, value.clone());
            }
            AssetOp::Lang(LangOp::Default { language, key, value }) => {
                self.defaults
                    .entry(language.clone())
                    .or_default()
                    .insert(key.clone(), value.clone());
            }
            AssetOp::Lang(LangOp::Table { language, source }) => {
                let path = resolve(&op, source)?;
                let bytes = std::fs::read(&path).map_err(io_err(&path))?;
                let read =
                    Lng::parse(&bytes).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
                let table = self.tables.entry(language.clone()).or_default();
                for e in read.entries {
                    table.set(&e.key, e.value);
                }
            }
            AssetOp::File(FileOp::Put { source, target }) => {
                if !relative_inside(target) {
                    return Err(Error::Resolve(format!(
                        "{target} must be a relative path inside the game folder"
                    )));
                }
                let src = resolve(&op, source)?;
                let rel = format!("{OVERLAY_DIR}/{target}");
                self.files.remove(&format!("{SWOFTY}/{rel}"));
                self.place(rel, src)?;
                self.assets.overlay = true;
            }
            AssetOp::Erp(_)
            | AssetOp::Db(_)
            | AssetOp::Track(_)
            | AssetOp::Item(_)
            | AssetOp::Warn { .. } => {}
        }
        Ok(())
    }

    // One overlay archive for each origin and set of target archives, so that each
    // addon or mod has its own, beside the ready ones in `prebuilt`. The game reads a
    // group's overlays in the listed order and the first that has a resource wins, so
    // they are listed from the last origin in `origins` to the first. An origin that
    // leaves a resource differing between two archives gets an overlay archive for
    // each, so every target reads the copy written for it.
    pub fn resources<'r>(
        &mut self,
        resources: impl IntoIterator<Item = (&'r str, &'r Resource, &'r Erp, &'r str)>,
        origins: &[String],
        prebuilt: &[Prebuilt],
        out: &Path,
    ) -> Result<Vec<String>> {
        let stale = out.join(SWOFTY).join(RESOURCES_DIR);
        if stale.is_dir() {
            std::fs::remove_dir_all(&stale).map_err(io_err(&stale))?;
        }
        let mut warnings = Vec::new();
        let rank = |origin: &str| origins.iter().position(|o| o == origin).unwrap_or(origins.len());
        let mut written = Vec::new();
        for (erp, res, stock, origin) in resources {
            if let Some(why) = layout_change(res, stock) {
                warnings.push(format!("runtime: {} in {erp} is left out: {why}", res.name));
                continue;
            }
            written.push((erp, res, stock, origin));
        }
        let parts = parts_of(&written);
        type Written<'w> = (&'w Resource, &'w Erp, BTreeSet<&'w str>);
        let mut by_uri: BTreeMap<(usize, &str, usize, &str), Written> = BTreeMap::new();
        for (erp, res, stock, origin) in written {
            let e = by_uri
                .entry((rank(origin), origin, parts[&(origin, erp)], &res.name))
                .or_insert((res, stock, BTreeSet::new()));
            e.0 = res;
            e.1 = stock;
            e.2.insert(erp);
        }
        // An origin's resources share one archive, which lists every target the
        // origin writes; only a second archive version or a differing copy of a
        // resource splits it. Resources without an origin keep one archive per set
        // of targets.
        type Set<'s> = (Vec<Resource>, &'s Erp, BTreeSet<&'s str>);
        let mut sets: BTreeMap<(usize, &str, usize, u32, Vec<&str>), Set> = BTreeMap::new();
        for ((n, origin, part, _), (res, stock, erps)) in by_uri {
            let split = match origin.is_empty() {
                true => erps.iter().copied().collect(),
                false => Vec::new(),
            };
            let set = sets.entry((n, origin, part, stock.version, split)).or_insert((
                Vec::new(),
                stock,
                BTreeSet::new(),
            ));
            set.0.push(res.clone());
            set.2.extend(erps);
        }
        let mut listed: Vec<(usize, ResErp)> = Vec::new();
        for (n, ((rank, origin, ..), (resources, stock, targets))) in sets.into_iter().enumerate() {
            let rel = format!("{RESOURCES_DIR}/{n}.erp");
            let path = out.join(SWOFTY).join(&rel);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            self.resources += resources.len();
            let mut erp = Erp {
                header: stock.header,
                version: stock.version,
                resources,
            };
            std::fs::write(&path, erp.to_bytes()?).map_err(io_err(&path))?;
            self.place(rel.clone(), path)?;
            listed.push((
                rank,
                ResErp {
                    file: rel,
                    erps: targets.into_iter().map(str::to_owned).collect(),
                    origin: origin.to_owned(),
                },
            ));
        }
        for (n, p) in prebuilt.iter().enumerate() {
            let rel = format!("{RESOURCES_DIR}/p{n}.erp");
            self.resources += p.resources;
            self.place(rel.clone(), p.path.clone())?;
            listed.push((
                rank(&p.origin),
                ResErp {
                    file: rel,
                    erps: p.erps.clone(),
                    origin: p.origin.clone(),
                },
            ));
        }
        listed.sort_by_key(|e| std::cmp::Reverse(e.0));
        self.assets
            .resource_erps
            .extend(listed.into_iter().map(|(_, e)| e));
        Ok(warnings)
    }

    pub fn db(&mut self, edits: Vec<DbEdit>) {
        self.assets.early_stage |= !edits.is_empty();
        self.assets.db_edits.extend(edits);
    }

    pub fn summary(&self) -> String {
        let a = &self.assets;
        let overlay = self
            .files
            .keys()
            .filter(|k| k.starts_with(&format!("{SWOFTY}/{OVERLAY_DIR}/")))
            .count();
        format!(
            "runtime: {ASSETS_FILE} with {} asset group(s), {} resource(s), {} database edit(s), {} language table(s), {overlay} overlay file(s)",
            a.asset_groups.len(),
            self.resources,
            a.db_edits.len(),
            a.lang.len(),
        )
    }

    pub fn finish(mut self, out: &Path) -> Result<Option<Self>> {
        for (language, defaults) in std::mem::take(&mut self.defaults) {
            let table = self.tables.entry(language).or_default();
            for (key, value) in defaults {
                if table.get(&key).is_none() {
                    table.set(&key, value);
                }
            }
        }
        for (language, table) in std::mem::take(&mut self.tables) {
            let rel = lang_file(&language);
            let path = out.join(SWOFTY).join(&rel);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&path, table.to_bytes()).map_err(io_err(&path))?;
            self.place(rel.clone(), path)?;
            self.assets.lang.insert(language, LangTable { table: rel });
        }
        if let Some(packed) = self.look.take() {
            let path = out.join(SWOFTY).join(SHADER_LUT);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&path, packed).map_err(io_err(&path))?;
            self.place(SHADER_LUT.into(), path)?;
        }
        for (i, dxil) in std::mem::take(&mut self.passes).into_iter().enumerate() {
            let rel = self.assets.postfx[i].shader.clone();
            let path = out.join(SWOFTY).join(&rel);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&path, dxil).map_err(io_err(&path))?;
            self.place(rel, path)?;
        }
        let files: Vec<_> = self
            .assets
            .postfx_textures
            .iter()
            .filter_map(|t| t.file.clone())
            .zip(std::mem::take(&mut self.images))
            .collect();
        for (rel, tex) in files {
            let path = out.join(SWOFTY).join(&rel);
            let dir = path.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(io_err(dir))?;
            std::fs::write(&path, tex).map_err(io_err(&path))?;
            self.place(rel, path)?;
        }
        let empty = Assets {
            features: self.assets.features.clone(),
            ..Assets::default()
        };
        if self.assets == empty && !self.enabled {
            return Ok(None);
        }
        let path = out.join(ASSETS_FILE);
        crate::write_json(&path, &self.assets)?;
        self.files.insert(ASSETS_FILE.into(), path);
        Ok(Some(self))
    }
}
