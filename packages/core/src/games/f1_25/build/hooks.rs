//! The build hooks of the addons on, each called on a worker as soon as the build knows its load order and
//! options, so a helper answers while the build checks and reads everything before it. A reply is taken at its
//! addon's place in the load order, so the build reads it exactly as it would a call made there.

use std::collections::BTreeMap;
use std::path::Path;
use std::thread::JoinHandle;

use super::run::Run;
use crate::addons::helper::HookRun;
use crate::addons::registry::{Entry, Host, Registry};
use crate::asset::{AssetBatch, Features};
use crate::launcher::BuildCx;
use crate::{Error, Result};

/// A hook's ops, lines and fields, and whether a helper ran its build or reused its kept reply.
pub(super) type Reply = (AssetBatch, Option<HookRun>);

/// The hook calls started for one build, by addon.
#[derive(Default)]
pub(super) struct Hooks {
    features: Features,
    calls: BTreeMap<String, JoinHandle<Result<Reply>>>,
}

impl Hooks {
    /// Starts the build hook of every addon in `run`'s order whose ops aren't all declared. An addon whose host
    /// can't be made, or whose worker can't start, is left to be called in place.
    pub fn start(run: Run, registry: &Registry, features: &Features) -> Self {
        let mut calls = BTreeMap::new();
        for id in run.order {
            let Some(entry) = registry.get(id).filter(|e| !e.declared) else {
                continue;
            };
            let Ok(host) = Host::new(run.store, id, run.settings, Some(run.game)) else {
                continue;
            };
            let host = host.read_only(run.checking);
            let options = options(run, id);
            let kept = super::inputs::hook_reply_path(run.store, run.profile, id);
            let (entry, features, addon) = (entry.clone(), features.clone(), id.clone());
            let worker = std::thread::Builder::new()
                .name(format!("hook {id}"))
                .spawn(move || call(&entry, &host, &options, &features, &kept, &addon));
            if let Ok(worker) = worker {
                calls.insert(id.clone(), worker);
            }
        }
        Self {
            features: features.clone(),
            calls,
        }
    }

    /// The reply of `id`'s hook if it was started for `features`, which a panic in it ends with; None leaves the
    /// call to be made in place.
    pub fn take(&mut self, id: &str, features: &Features) -> Option<Result<Reply>> {
        let worker = self.calls.remove(id)?;
        let reply = worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        (self.features == *features).then_some(reply)
    }
}

impl Drop for Hooks {
    /// Waits for the calls a build that stopped early never took, as it would have waited for a call in place.
    fn drop(&mut self) {
        for (_, worker) in std::mem::take(&mut self.calls) {
            worker.join().ok();
        }
    }
}

/// The values of `id`'s options the build's profile gives.
pub(super) fn options(run: Run, id: &str) -> BTreeMap<String, serde_json::Value> {
    let m = &run.addons[id];
    m.options
        .iter()
        .map(|o| (o.id.clone(), run.on.value(run.settings, m, o)))
        .collect()
}

/// Calls `id`'s build hook: through the helper's kept reply for a helper, or the addon's own build.
pub(super) fn call(
    entry: &Entry,
    host: &Host,
    options: &BTreeMap<String, serde_json::Value>,
    features: &Features,
    kept: &Path,
    id: &str,
) -> Result<Reply> {
    let cx = BuildCx {
        cx: host.cx(),
        options,
        features,
    };
    match &entry.plugin {
        Some(plugin) => plugin.build_kept(&cx, kept),
        None => entry.addon.build(&cx).map(|batch| (batch, None)),
    }
    .map_err(|e| Error::Other(format!("building {id}: {e}")))
}
