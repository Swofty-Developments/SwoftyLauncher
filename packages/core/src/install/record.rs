//! What an install records before it changes anything: the journal's steps, the placed record and
//! the user.reg override.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::files::rel_parts;
use super::wine_override::add_override;
use super::{Installer, Plan};
use crate::game_folder::baseline::{Baseline, Objects, sha256_bytes};
use crate::game_folder::journal::{Journal, Op};
use crate::game_folder::placed::Placed;
use crate::{Result, io_err};

/// One journal step: what it does, to which game path, and the file's hash
/// before and after.
pub(super) type Step = (Op, String, Option<String>, Option<String>);

/// The user.reg an install overrides, with its text before and after.
pub(super) type RegistryEdit<'a> = (&'a PathBuf, String, String);

impl<'a> Installer<'a> {
    /// Opens the journal of `plan`'s install with every step pending, after
    /// recording what it places and the user.reg it edits. Returns the
    /// journal and that user.reg with its text before and after the override.
    pub(super) fn open_journal(
        &self,
        plan: &Plan,
        baseline: &Baseline,
        objects: &Objects,
        placed: &mut Placed,
        after: &BTreeMap<&str, String>,
    ) -> Result<(Journal, Option<RegistryEdit<'a>>)> {
        let mut j = Journal::create(
            self.data,
            &plan.profile,
            &self.game.identity.build_id,
            self.session,
        )?;
        j.game = Some(self.game.dir.clone());
        for rel in plan.data.keys() {
            placed.insert(rel, after[rel.as_str()].clone(), true);
        }
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            placed.insert(rel, after[rel.as_str()].clone(), false);
        }
        let registry = self.registry_edit(plan, placed)?;
        if let Some((_, name, _, _)) = &registry {
            placed.registry = Some(name.clone());
        }
        placed.save(self.data)?;
        let mut steps = self.steps(plan, baseline, after);
        let registry = match registry {
            Some((reg, name, text, out)) => {
                let (before, now) = (objects.put_file(reg)?, sha256_bytes(out.as_bytes()));
                steps.push((Op::RegistryOverride, name, Some(before), Some(now)));
                Some((reg, text, out))
            }
            None => None,
        };
        j.begin_all(steps)?;
        Ok((j, registry))
    }

    /// The user.reg `plan` overrides, its name in the journal, and its text
    /// now and with the override; one that has the override already counts
    /// only when the launcher put it there.
    pub(super) fn registry_edit(
        &self,
        plan: &Plan,
        placed: &Placed,
    ) -> Result<Option<(&'a PathBuf, String, String, String)>> {
        let game = self.game;
        Ok(match &game.user_reg {
            Some(reg) if plan.registry_override => {
                let text = std::fs::read_to_string(reg).map_err(io_err(reg))?;
                let name = reg.display().to_string();
                let ours = placed.registry.as_deref() == Some(name.as_str());
                match add_override(&text) {
                    Some(out) => Some((reg, name, text, out)),
                    None => ours.then(|| (reg, name, text.clone(), text)),
                }
            }
            _ => None,
        })
    }

    /// The journal steps that place `plan`'s files: each replacement, then
    /// each new file after the folders it needs that are not there yet.
    pub(super) fn steps(
        &self,
        plan: &Plan,
        baseline: &Baseline,
        after: &BTreeMap<&str, String>,
    ) -> Vec<Step> {
        let mut steps = Vec::new();
        for rel in plan.data.keys() {
            let before = baseline.files[rel].sha256.clone();
            steps.push((
                Op::ReplaceFile,
                rel.clone(),
                Some(before),
                Some(after[rel.as_str()].clone()),
            ));
        }
        let mut dirs = BTreeSet::new();
        for rel in plan.added.keys().chain(plan.loader.keys()) {
            for dir in rel_parts(rel) {
                if !self.path(&dir).is_dir() && dirs.insert(dir.clone()) {
                    steps.push((Op::CreateDir, dir, None, None));
                }
            }
            steps.push((
                Op::CreateFile,
                rel.clone(),
                None,
                Some(after[rel.as_str()].clone()),
            ));
        }
        steps
    }
}
