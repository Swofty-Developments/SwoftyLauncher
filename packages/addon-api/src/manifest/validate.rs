use std::collections::BTreeMap;
use std::path::Path;

use crate::manifest::compat::Platform;
use crate::manifest::hud::{HudMfd, MFD_ROWS_MAX, MFD_TEXT_MAX};
use crate::manifest::kind::ExperimentalFeature;
use crate::manifest::options::{Binds, OptionType};
use crate::manifest::postfx::{
    POSTFX_FORMATS, POSTFX_INPUTS, POSTFX_OPTIONS, POSTFX_RESERVED, POSTFX_SKIPS, POSTFX_VIEWS, PassValue,
    PostFx, PostFxTexture, is_identifier, placeholders, postfx_key_ok, texture_name_ok,
};
use crate::manifest::shader::{Shader, Strength};
use crate::manifest::{AddonManifest, SCHEMA_VERSION};
use crate::{Error, Result};

pub fn valid_screen_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `url` when it is a link the launcher may open: `https://` with something after it.
pub fn https_url(url: &str) -> Option<&str> {
    let url = url.trim();
    url.strip_prefix("https://")
        .filter(|rest| !rest.is_empty())
        .map(|_| url)
}

impl AddonManifest {
    pub fn validate(&self) -> Result<()> {
        let fail = |m: String| Err(Error::Manifest(format!("{}: {m}", self.id)));
        if self.schema_version != SCHEMA_VERSION {
            return fail(format!("schema_version {} is not supported", self.schema_version));
        }
        if !valid_id(&self.id) {
            return fail("id must be lowercase dotted [a-z0-9._-] with at least two parts".into());
        }
        if self.compat.game != crate::GAME_ID {
            return fail(format!(
                "compat.game {} is not {}",
                self.compat.game,
                crate::GAME_ID
            ));
        }
        for p in &self.compat.platforms {
            if Platform::parse(p).is_none() {
                return fail(format!("compat.platforms {p:?} is not one of \"steam\", \"ea\""));
            }
        }
        let mut seen = BTreeMap::new();
        for o in &self.options {
            if seen.insert(o.id.as_str(), ()).is_some() {
                return fail(format!("option {} is declared twice", o.id));
            }
            if let Err(e) = o.validate() {
                return fail(format!("option {}: {e}", o.id));
            }
            if o.needs.iter().any(|n| n.id == self.id) {
                return fail(format!("option {}: needs names the addon itself", o.id));
            }
        }
        let mut screens = BTreeMap::new();
        for s in &self.operations.screens {
            if !valid_screen_name(&s.name) || !valid_screen_name(&s.flow) {
                return fail(format!("screen {}: name and flow must be [a-z0-9_]", s.name));
            }
            if s.anchor.as_deref().is_some_and(|a| !valid_screen_name(a)) {
                return fail(format!("screen {}: anchor must be [a-z0-9_]", s.name));
            }
            if screens.insert(s.name.as_str(), ()).is_some() {
                return fail(format!("screen {} is declared twice", s.name));
            }
            match s.page.as_deref() {
                None if s.file.is_empty() => {
                    return fail(format!("screen {}: needs a file, or page: options", s.name));
                }
                Some(p) if p != "options" => {
                    return fail(format!("screen {}: page must be options, not {p}", s.name));
                }
                _ => {}
            }
        }
        let paths = self
            .operations
            .database
            .iter()
            .chain(&self.operations.erp)
            .chain(&self.operations.ui)
            .chain(&self.operations.hud)
            .chain(&self.operations.assets);
        let paths = paths
            .chain(self.operations.files.iter().map(|f| &f.source))
            .chain(
                self.operations
                    .native
                    .iter()
                    .flat_map(|n| std::iter::once(&n.dll).chain(&n.files)),
            )
            .chain(
                self.operations
                    .screens
                    .iter()
                    .filter(|s| s.page.is_none())
                    .map(|s| &s.file),
            )
            .chain(self.launcher.iter().flat_map(|l| l.program.values()))
            .chain(&self.icon);
        if let Some(p) = paths.into_iter().find(|p| !relative_inside(p)) {
            return fail(format!("{p} must be a relative path inside the addon"));
        }
        for (i, s) in self.shaders.iter().enumerate() {
            if !relative_inside(&s.config) || !s.config.ends_with(".swfx") {
                return fail(format!(
                    "shaders[{i}]: {} must be a relative .swfx path inside the addon",
                    s.config
                ));
            }
            if self.shaders[..i].iter().any(|o| o.stem() == s.stem()) {
                return fail(format!(
                    "shaders[{i}]: a config named {} is listed twice",
                    s.stem()
                ));
            }
        }
        if self.shaders.iter().filter(|s| s.force).count() > 1 {
            return fail("shaders: only one config may be forced".into());
        }
        for (i, e) in self.effects.iter().enumerate() {
            if !relative_inside(e) || !e.ends_with(".effect.json") {
                return fail(format!(
                    "effects[{i}]: {e} must be a relative .effect.json path inside the addon"
                ));
            }
        }
        if let Some(c) = &self.career_setup {
            c.validate(&self.options)
                .or_else(|e| fail(format!("career_setup: {e}")))?;
        }
        if let Some(h) = &self.hub_tab {
            h.validate(&self.options)
                .or_else(|e| fail(format!("hub_tab: {e}")))?;
        }
        if let Some(s) = &self.shader {
            self.validate_shader(s)
                .or_else(|e| fail(format!("shader: {e}")))?;
        }
        for (i, p) in self.postfx.iter().enumerate() {
            self.validate_postfx(p)
                .or_else(|e| fail(format!("postfx[{i}]: {e}")))?;
        }
        for (i, t) in self.postfx_textures.iter().enumerate() {
            self.validate_postfx_texture(t, i)
                .or_else(|e| fail(format!("postfx_textures[{i}]: {e}")))?;
        }
        if let Some(t) = &self.postfx_toggle {
            self.validate_postfx_toggle(t)
                .or_else(|e| fail(format!("postfx_toggle: {e}")))?;
        }
        if let Some(m) = self.hud.as_ref().and_then(|h| h.mfd.as_ref()) {
            self.validate_mfd(m).or_else(|e| fail(format!("hud.mfd: {e}")))?;
        }
        if let Some(l) = &self.launcher {
            if l.program.is_empty() {
                return fail("launcher.program names no program".into());
            }
            if let Some(e) = l.imports.iter().find(|e| e.is_empty() || e.contains('.')) {
                return fail(format!(
                    "launcher.imports: {e:?} must be an extension without the dot"
                ));
            }
        }
        Ok(())
    }

    fn validate_postfx(&self, p: &PostFx) -> std::result::Result<(), String> {
        if !self.experimental_features.contains(&ExperimentalFeature::Postfx) {
            return Err("list \"postfx\" in experimental_features to use it".into());
        }
        if !relative_inside(&p.shader) {
            return Err(format!(
                "shader {} must be a relative path inside the addon",
                p.shader
            ));
        }
        if !p.shader.to_ascii_lowercase().ends_with(".hlsl") {
            return Err(format!("shader {} must be a .hlsl file", p.shader));
        }
        if p.options.len() > POSTFX_OPTIONS {
            return Err(format!(
                "{} options is more than {POSTFX_OPTIONS}",
                p.options.len()
            ));
        }
        if let Some(b) = p.budget_ms
            && !(b > 0.0 && b <= 16.0)
        {
            return Err(format!("budget_ms {b} is not above 0 and at most 16"));
        }
        for v in &p.options {
            let PassValue::Option(t) = v else { continue };
            let [id] = placeholders(t)[..] else {
                return Err(format!("option {t:?} must be a number or one {{{{option}}}}"));
            };
            if !(t.trim().starts_with("{{") && t.trim().ends_with("}}")) {
                return Err(format!("option {t:?} must be a number or one {{{{option}}}}"));
            }
            if !self.options.iter().any(|o| o.id == id) {
                return Err(format!("{{{{{id}}}}} names no option"));
            }
        }
        if let Some(e) = &p.entry
            && !is_identifier(e)
        {
            return Err(format!("entry {e:?} must be an identifier"));
        }
        match (p.raw, p.threads) {
            (true, None) => return Err("a raw pass needs threads".into()),
            (false, Some(_)) => return Err("threads is for a raw pass".into()),
            (true, Some([x, y])) if x == 0 || y == 0 || x > 1024 || y > 1024 || x * y > 1024 => {
                return Err(format!("threads [{x}, {y}] must be 1 to 1024 in total"));
            }
            _ => {}
        }
        if p.inputs.len() > POSTFX_INPUTS {
            return Err(format!("{} inputs is more than {POSTFX_INPUTS}", p.inputs.len()));
        }
        for n in &p.inputs {
            if n != "original" && n != "previous" && !self.postfx_textures.iter().any(|t| &t.name == n) {
                return Err(format!("input {n} names no postfx_textures entry"));
            }
        }
        if let Some(n) = &p.output {
            match self.postfx_textures.iter().find(|t| &t.name == n) {
                None => return Err(format!("output {n} names no postfx_textures entry")),
                Some(t) if t.file.is_some() => {
                    return Err(format!("output {n} is an image, which is read-only"));
                }
                Some(_) => {}
            }
        }
        if let Some(w) = &p.when {
            if let Some(v) = w.views.iter().find(|v| !POSTFX_VIEWS.contains(&v.as_str())) {
                return Err(format!(
                    "when.views {v:?} is not one of {}",
                    POSTFX_VIEWS.join(", ")
                ));
            }
            if let Some(v) = w.skip.iter().find(|v| !POSTFX_SKIPS.contains(&v.as_str())) {
                return Err(format!(
                    "when.skip {v:?} is not one of {}",
                    POSTFX_SKIPS.join(", ")
                ));
            }
        }
        if let Some(t) = &p.enabled {
            let id = self.single_placeholder(t, "enabled")?;
            match self.options.iter().find(|o| o.id == id) {
                None => return Err(format!("{{{{{id}}}}} names no option")),
                Some(o) if o.kind != OptionType::Toggle => {
                    return Err(format!("enabled option {id} must be a toggle"));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn single_placeholder<'a>(&self, t: &'a str, what: &str) -> std::result::Result<&'a str, String> {
        match placeholders(t)[..] {
            [id] if t.trim().starts_with("{{") && t.trim().ends_with("}}") => Ok(id),
            _ => Err(format!("{what} {t:?} must be one {{{{option}}}}")),
        }
    }

    fn validate_postfx_texture(&self, t: &PostFxTexture, at: usize) -> std::result::Result<(), String> {
        if !self.experimental_features.contains(&ExperimentalFeature::Postfx) {
            return Err("list \"postfx\" in experimental_features to use it".into());
        }
        if !texture_name_ok(&t.name) {
            return Err(format!("name {:?} must be 1 to 32 of a-z, 0-9 and _", t.name));
        }
        if POSTFX_RESERVED.contains(&t.name.as_str()) {
            return Err(format!("name {} is reserved", t.name));
        }
        if self.postfx_textures[..at].iter().any(|o| o.name == t.name) {
            return Err(format!("name {} is listed twice", t.name));
        }
        if let Some(f) = &t.format
            && !POSTFX_FORMATS.contains(&f.as_str())
        {
            return Err(format!(
                "format {f:?} is not one of {}",
                POSTFX_FORMATS.join(", ")
            ));
        }
        let sized = t.width.is_some() || t.height.is_some();
        match (&t.file, t.scale, sized) {
            (Some(f), None, false) => {
                if !relative_inside(f) {
                    return Err(format!("file {f} must be a relative path inside the addon"));
                }
                if !f.to_ascii_lowercase().ends_with(".png") {
                    return Err(format!("file {f} must be a .png file"));
                }
                if t.persistent || t.format.is_some() {
                    return Err("an image texture takes no format or persistent".into());
                }
            }
            (None, Some(s), false) => {
                if !(s > 0.0 && s <= 2.0) {
                    return Err(format!("scale {s} is not above 0 and at most 2"));
                }
            }
            (None, None, true) => match (t.width, t.height) {
                (Some(w), Some(h)) if (1..=8192).contains(&w) && (1..=8192).contains(&h) => {}
                (Some(_), Some(_)) => return Err("width and height must be 1 to 8192".into()),
                _ => return Err("width and height go together".into()),
            },
            _ => return Err("give exactly one of scale, width and height, or file".into()),
        }
        Ok(())
    }

    fn validate_postfx_toggle(&self, t: &str) -> std::result::Result<(), String> {
        if !self.experimental_features.contains(&ExperimentalFeature::Postfx) {
            return Err("list \"postfx\" in experimental_features to use it".into());
        }
        let id = self.single_placeholder(t, "value")?;
        let Some(o) = self.options.iter().find(|o| o.id == id) else {
            return Err(format!("{{{{{id}}}}} names no option"));
        };
        if o.binds != Binds::Native || o.kind != OptionType::Choice {
            return Err(format!("option {id} must be a native choice"));
        }
        if let Some(c) = o.values.iter().find(|c| !postfx_key_ok(&c.value)) {
            return Err(format!("option {id} value {:?} is not a key name", c.value));
        }
        Ok(())
    }

    fn validate_mfd(&self, m: &HudMfd) -> std::result::Result<(), String> {
        if m.rows.is_empty() || m.rows.len() > MFD_ROWS_MAX {
            return Err(format!("rows must list 1 to {MFD_ROWS_MAX} options"));
        }
        if let Some(t) = &m.title
            && (t.trim().is_empty() || t.chars().count() > MFD_TEXT_MAX)
        {
            return Err(format!("title must be 1 to {MFD_TEXT_MAX} characters"));
        }
        let mut seen = BTreeMap::new();
        for r in &m.rows {
            if seen.insert(r.as_str(), ()).is_some() {
                return Err(format!("row {r} is listed twice"));
            }
            let Some(o) = self.options.iter().find(|o| &o.id == r) else {
                return Err(format!("row {r} names no option of this addon"));
            };
            if o.kind == OptionType::Slider {
                return Err(format!("row {r}: only toggle and choice options can be MFD rows"));
            }
            if o.label.as_deref().unwrap_or(&o.id).chars().count() > MFD_TEXT_MAX {
                return Err(format!(
                    "row {r}: the option label is longer than {MFD_TEXT_MAX} characters"
                ));
            }
        }
        Ok(())
    }

    fn validate_shader(&self, s: &Shader) -> std::result::Result<(), String> {
        if !self.experimental_features.contains(&ExperimentalFeature::Shaders) {
            return Err("list \"shaders\" in experimental_features to use it".into());
        }
        if !relative_inside(&s.lut) {
            return Err(format!("lut {} must be a relative path inside the addon", s.lut));
        }
        if !s.lut.to_ascii_lowercase().ends_with(".cube") {
            return Err(format!("lut {} must be a .cube file", s.lut));
        }
        if s.lut.matches("{{").count() != placeholders(&s.lut).len() {
            return Err(format!("lut {} has an unclosed {{{{", s.lut));
        }
        let db_option = |id: &str, slider: bool| {
            let o = self.options.iter().find(|o| o.id == id);
            match o {
                None => Err(format!("{{{{{id}}}}} names no option")),
                Some(o) if o.binds != Binds::Db => Err(format!("option {id} must bind db")),
                Some(o) if slider && o.kind != OptionType::Slider => {
                    Err(format!("option {id} must be a slider"))
                }
                Some(o) if slider && (o.min.unwrap_or(0.) < 0. || o.max.unwrap_or(0.) > 100.) => {
                    Err(format!("option {id} must stay within 0 to 100"))
                }
                Some(_) => Ok(()),
            }
        };
        for id in placeholders(&s.lut) {
            db_option(id, false)?;
        }
        match &s.strength {
            Some(Strength::Percent(p)) if !(0.0..=100.0).contains(p) => {
                Err(format!("strength {p} is not from 0 to 100"))
            }
            Some(Strength::Option(t)) => match placeholders(t)[..] {
                [id] if t.trim().starts_with("{{") && t.trim().ends_with("}}") => db_option(id, true),
                _ => Err(format!("strength {t:?} must be a number or one {{{{option}}}}")),
            },
            _ => Ok(()),
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        })
}

pub fn relative_inside(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && !path.contains('\\')
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
}
