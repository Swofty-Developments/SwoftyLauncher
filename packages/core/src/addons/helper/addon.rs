//! A helper addon as the launcher's Addon and Importer.

use std::path::{Path, PathBuf};
use std::sync::TryLockError;
use std::time::SystemTime;

use swofty_addon::asset::{AssetBatch, Op, Origin};
use swofty_addon::helper::{Built, Call, RESTART};
use swofty_addon::launcher::{Addon, BuildCx, Claim, CommandSpec, Cx, Importer, Progress, Summary};
use swofty_addon::manifest::AddonManifest;
use swofty_addon::view::{Effect, Event, SectionSpec, View};

use super::protocol::Run;
use super::{HelperAddon, HookRun, Result, reads};

impl Addon for HelperAddon {
    fn manifest(&self) -> &AddonManifest {
        &self.manifest
    }

    fn sections(&self) -> Vec<SectionSpec> {
        self.spec.sections.clone()
    }

    fn view(&self, cx: &Cx, section: &str) -> Result<View> {
        if let Some(e) = self.failure() {
            return Ok(self.dead_view(e));
        }
        let call = Call::View {
            section: section.into(),
        };
        self.call(cx, call, self.timeouts.quick, None)
            .or_else(|e| self.failure().map(|e| self.dead_view(e)).ok_or(e))
    }

    fn event(&self, cx: &Cx, section: &str, event: Event) -> Result<Effect> {
        if matches!(&event, Event::Click { id } if id == RESTART) {
            self.stop();
            return Ok(Effect::Reload);
        }
        let call = Call::Event {
            section: section.into(),
            event,
        };
        self.call(cx, call, self.timeouts.long, None)
    }

    fn importer(&self) -> Option<&dyn Importer> {
        (self.spec.claims || !self.spec.imports.is_empty()).then_some(self as &dyn Importer)
    }

    fn build(&self, cx: &BuildCx) -> Result<AssetBatch> {
        if !self.spec.build {
            return Ok(AssetBatch::default());
        }
        let built: Built = self.call(&cx.cx, build_call(cx), self.timeouts.long, None)?;
        Ok(self.batch(built))
    }

    fn commands(&self) -> Vec<CommandSpec> {
        self.spec.commands.clone()
    }

    fn command(&self, cx: &Cx, path: &[String], args: &[String]) -> Result<String> {
        let call = Call::Command {
            path: path.to_vec(),
            args: args.to_vec(),
        };
        self.call(cx, call, self.timeouts.long, None)
    }

    fn summary(&self, cx: &Cx) -> Option<Summary> {
        let mut last = self.summaries.lock().unwrap_or_else(|e| e.into_inner());
        let run = match self.run.try_lock() {
            Ok(run) => run,
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
            Err(TryLockError::WouldBlock) => return last.get(&cx.profile.name).cloned().flatten(),
        };
        if !matches!(*run, Run::Live(_)) {
            last.clear();
            return None;
        }
        let summary = self
            .call_with(run, cx, Call::Summary, self.timeouts.quick, None)
            .ok()
            .flatten();
        last.insert(cx.profile.name.clone(), summary.clone());
        summary
    }
}

fn build_call(cx: &BuildCx) -> Call {
    Call::Build {
        options: cx.options.clone(),
        features: cx.features.clone(),
    }
}

impl HelperAddon {
    fn batch(&self, built: Built) -> AssetBatch {
        let mut items = built.items.into_iter();
        AssetBatch {
            ops: built
                .ops
                .into_iter()
                .map(|op| Op {
                    op,
                    origin: Origin {
                        addon: self.manifest.id.clone(),
                        item: items.next().flatten(),
                    },
                })
                .collect(),
            lines: built.lines,
            warnings: built.warnings,
            fields: built.fields,
            reads: built.reads,
        }
    }

    /// The build hook's ops, answered with the reply kept at `kept` while nothing it depends on changed,
    /// else from the helper, whose reply is then kept there; and whether the helper ran, and why.
    pub fn build_kept(&self, cx: &BuildCx, kept: &Path) -> Result<(AssetBatch, Option<HookRun>)> {
        if !self.spec.build {
            return Ok((AssetBatch::default(), None));
        }
        let call = build_call(cx);
        let asking = reads::Asking::new(&super::hello(&cx.cx), &call);
        let folders = reads::Folders {
            own: &self.dir,
            data: cx.cx.data_dir,
        };
        let why = match reads::reuse(kept, &asking, &folders, &cx.cx) {
            Ok((built, why)) => return Ok((self.batch(built), Some(HookRun::Reused(why)))),
            Err(why) => why,
        };
        let started = SystemTime::now();
        let mut asked = Vec::new();
        let built: Result<Built> = self.call_asking(
            self.lock(),
            &cx.cx,
            call,
            self.timeouts.long,
            None,
            Some(&mut asked),
        );
        match built {
            Ok(built) => {
                reads::keep(kept, asking, &built, asked, started, &folders, self.settled);
                Ok((self.batch(built), Some(HookRun::Ran(why))))
            }
            Err(e) => {
                reads::forget(kept);
                Err(e)
            }
        }
    }
}

impl Importer for HelperAddon {
    fn claims(&self, path: &Path) -> Claim {
        if self.spec.claims
            && let Some(claim) = self.ask_claims(path)
        {
            return claim;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        if path.is_file() && self.spec.imports.iter().any(|i| i.eq_ignore_ascii_case(ext)) {
            Claim::Yes
        } else {
            Claim::No
        }
    }

    fn import(&self, cx: &Cx, paths: &[PathBuf], progress: &dyn Progress) -> Result<Vec<String>> {
        let call = Call::Import {
            paths: paths.to_vec(),
        };
        self.call(cx, call, self.timeouts.long, Some(progress))
    }
}
