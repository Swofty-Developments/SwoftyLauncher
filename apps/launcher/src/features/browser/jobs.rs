use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::shell::notices::Notification;
use gpui_kit::*;
use swofty_core::catalog::{self, Addon};
use swofty_core::manifest::AddonManifest;
use swofty_core::updates::release::current_version;
use swofty_i18n::trf;

use super::state::BrowserChange;
use super::*;
use crate::services::library::Library;
use crate::shell::host::Host;
use crate::ui::format;

pub(super) fn install_failure_toast(name: &str, verb: Verb, refused: bool, error: String) -> Notification {
    Notification::error(error).title(verb.failure_title(name, refused))
}

struct Finished<'a> {
    addon: &'a Addon,
    version: &'a catalog::Version,
    verb: Verb,
    took: Duration,
    in_batch: bool,
}

pub(super) fn requested_version_available(addon: Option<&Addon>, requested: &catalog::Version) -> bool {
    addon.is_some_and(|addon| {
        addon.listed().iter().any(|version| {
            version.version == requested.version
                && version.accepts(&current_version())
                && version.sha256 == requested.sha256
                && version.size == requested.size
                && version.url == requested.url
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RetryAction {
    Retry,
    RequestCurrent,
    Unavailable,
}

pub(super) fn retry_action(addon: Option<&Addon>, requested: &catalog::Version) -> RetryAction {
    if requested_version_available(addon, requested) {
        RetryAction::Retry
    } else if addon.and_then(|a| a.offered(&current_version())).is_some() {
        RetryAction::RequestCurrent
    } else {
        RetryAction::Unavailable
    }
}

/// The addons an install brings along that no other install is already bringing.
pub(super) fn riders(
    preview: Vec<String>,
    working: &HashMap<String, Job>,
    batch: Option<&Batch>,
) -> Vec<String> {
    preview
        .into_iter()
        .filter(|d| !working.contains_key(d) && !batch.is_some_and(|b| b.left.contains(d)))
        .collect()
}

/// The version of `id` installed in `library`, if any.
pub(crate) fn installed_version(library: &Library, id: &str) -> Option<semver::Version> {
    library
        .addons
        .iter()
        .find(|m| m.manifest.id == id)
        .map(|m| m.manifest.version.clone())
}

impl Browser {
    pub(crate) fn install<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) {
        Self::enqueue(host, id, cx);
    }

    /// True when a job was queued or an existing pending job will handle it.
    pub(super) fn enqueue<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) -> bool {
        let (b, needs) = host.split();
        if needs.locked {
            return false;
        }
        let Some(addon) = b.find(id).cloned() else {
            return false;
        };
        let Some(version) = addon.offered(&current_version()).cloned() else {
            return false;
        };
        if needs.building.is_some_and(|busy| busy.holds(id)) {
            b.toasts
                .push(Notification::warning(crate::features::update::BUILDING));
            return false;
        }
        if let Some(existing) = b.queue.jobs().iter().find(|job| {
            job.id == id
                && matches!(
                    job.state,
                    crate::services::download_queue::State::Queued
                        | crate::services::download_queue::State::Active
                )
        }) {
            return existing.request.1 == version;
        }
        let claims = match b.requirements(needs.library, &addon) {
            Some(Ok(plan)) => plan.also().iter().map(|s| s.addon.id.clone()).collect(),
            _ => BTreeSet::new(),
        };
        let profile = needs.library.profile.name.clone();
        if b.queue.enqueue(id.to_owned(), (addon, version, profile), claims) {
            Self::start_queued(host, cx);
        }
        true
    }

    pub(super) fn start_queued<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (b, needs) = host.split();
        let queued: Vec<_> = b
            .queue
            .jobs()
            .iter()
            .filter(|job| job.state == crate::services::download_queue::State::Queued)
            .map(|job| (job.id.clone(), job.request.0.clone()))
            .collect();
        for (id, addon) in queued {
            if let Some(Ok(plan)) = b.requirements(needs.library, &addon) {
                b.queue
                    .update_claims(&id, plan.also().iter().map(|s| s.addon.id.clone()).collect());
            }
        }
        loop {
            let (b, needs) = host.split();
            let Some(id) = b.queue.start_next() else {
                break;
            };
            let (addon, version, profile) = b
                .queue
                .jobs()
                .iter()
                .find(|job| job.id == id && job.state == crate::services::download_queue::State::Active)
                .expect("the scheduler just activated this job")
                .request
                .clone();
            if requested_version_available(b.find(&id), &version) {
                Self::start_install(host, addon, version, profile, cx);
                continue;
            }
            let error = trf!(
                "{name} {version} is no longer available or compatible; request it again.",
                name = addon.name,
                version = version.version
            );
            b.queue.finish(&id, Err(error.clone()));
            b.failed.insert(id.clone(), error.clone());
            if let Some(batch) = &mut b.batch
                && batch.left.remove(&id)
            {
                batch.failed.push((addon.name.clone(), error));
            } else {
                let verb = Verb::of(installed_version(needs.library, &id).is_some());
                b.toasts
                    .push(install_failure_toast(&addon.name, verb, false, error));
            }
            if b.batch.as_ref().is_some_and(|batch| batch.left.is_empty()) {
                b.finish_updates(&needs.store.root);
            }
        }
        cx.notify();
    }

    fn start_install<H: Host<Self>>(
        host: &mut H,
        addon: Addon,
        version: catalog::Version,
        profile: String,
        cx: &mut Context<H>,
    ) {
        let (b, needs) = host.split();
        let id = addon.id.as_str();
        let from = installed_version(needs.library, id);
        let updating = from.is_some();
        b.failed.remove(id);
        b.refused.remove(id);
        let job = Job::new(version.size);
        let preview: Vec<String> = match b.requirements(needs.library, &addon) {
            Some(Ok(plan)) => plan.also().iter().map(|s| s.addon.id.clone()).collect(),
            _ => Vec::new(),
        };
        let riders = riders(preview, &b.working, b.batch.as_ref());
        for d in &riders {
            b.working.insert(d.clone(), job.clone());
        }
        b.working.insert(addon.id.clone(), job.clone());
        catalog::log(
            &needs.store.root,
            &format!(
                "{} {} {} ({}) from {}",
                if updating { "updating" } else { "installing" },
                addon.id,
                version.version,
                format::size(version.size),
                version.url
            ),
        );
        let pending =
            catalog::pending::Pending::begin(needs.store, &addon.id, &version.version, from.as_ref());
        let store = needs.store.clone();
        let catalog_now = b.addons.clone();
        let started = Instant::now();
        let tracked = job.clone();
        cx.spawn(async move |this, cx| {
            let (target, v) = (addon.clone(), version.clone());
            let result = cx
                .background_executor()
                .spawn(async move {
                    catalog::requires::install_in_profile(&store, &catalog_now, &target, &v, &profile, &|e| {
                        job.track(&target.id, e)
                    })
                })
                .await;
            this.update(cx, |host, cx| {
                let (b, needs) = host.split();
                for d in &riders {
                    b.working.remove(d);
                }
                let finished = Finished {
                    addon: &addon,
                    version: &version,
                    verb: Verb::of(updating),
                    took: started.elapsed(),
                    in_batch: false,
                };
                let turn_on = b.took_install(&needs.store.root, finished, result, &tracked);
                pending.end();
                host.changed(BrowserChange::Installed, cx);
                if let Some((id, note)) = turn_on {
                    host.changed(BrowserChange::TurnOn { id, note }, cx);
                }
                Self::start_queued(host, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self::tick(host, cx);
        cx.notify();
    }

    /// Takes a finished install; an addon installed outside an update batch comes back with the
    /// notice that offers to turn it on.
    fn took_install(
        &mut self,
        root: &Path,
        mut f: Finished<'_>,
        result: swofty_core::Result<Vec<AddonManifest>>,
        tracked: &Job,
    ) -> Option<(String, Notification)> {
        let id = &f.addon.id;
        self.working.remove(id);
        self.queue.finish(
            id,
            result
                .as_ref()
                .map(|_| ())
                .map_err(|error| tracked.failure(&error.to_string())),
        );
        f.in_batch = self.batch.as_mut().is_some_and(|b| b.left.remove(id));
        let turn_on = match result {
            Ok(done) => self.install_succeeded(root, &f, &done),
            Err(e) => {
                self.install_failed(root, &f, e);
                None
            }
        };
        if f.in_batch {
            let error = self.failed.get(id).cloned().unwrap_or_default();
            self.held.insert(id.clone(), (f.version.version.clone(), error));
            if self.batch.as_ref().is_some_and(|b| b.left.is_empty()) {
                self.finish_updates(root);
            }
        }
        turn_on
    }

    fn install_succeeded(
        &mut self,
        root: &Path,
        f: &Finished<'_>,
        done: &[AddonManifest],
    ) -> Option<(String, Notification)> {
        let (m, also) = done.split_last().expect("an install ends with its addon");
        let title = f.verb.done(&m.name, &m.version);
        let also: Vec<String> = also.iter().map(|d| format!("{} {}", d.name, d.version)).collect();
        let mut detail = format!("{} · {}", format::size(f.version.size), f.addon.id);
        if !also.is_empty() {
            detail.push_str(&format!(
                " · {}",
                trf!("also installed {names}", names = also.join(", "))
            ));
        }
        self.record(root, Op::Install, true, title.clone(), detail, f.took);
        if f.in_batch {
            if let Some(b) = self.batch.as_mut() {
                b.updated.push(format!("{} {}", m.name, m.version));
            }
            return None;
        }
        let note = match also.is_empty() {
            true => Notification::success(title),
            false => {
                Notification::success(trf!("Also installed {names}", names = also.join(", "))).title(title)
            }
        };
        Some((m.id.clone(), note))
    }

    fn install_failed(&mut self, root: &Path, f: &Finished<'_>, e: swofty_core::Error) {
        let refused = matches!(e, swofty_core::Error::Requires(_));
        let error = e.to_string();
        crate::app::app_log::action(f.verb.action())
            .addon(&f.addon.id)
            .failed(&error);
        self.record(
            root,
            Op::Install,
            false,
            f.verb.failed(&f.addon.name, &f.version.version),
            error.clone(),
            f.took,
        );
        self.failed.insert(f.addon.id.clone(), error.clone());
        if refused {
            self.refused.insert(f.addon.id.clone());
        }
        if f.in_batch {
            if let Some(b) = self.batch.as_mut() {
                b.failed.push((f.addon.name.clone(), error));
            }
        } else {
            self.toasts
                .push(install_failure_toast(&f.addon.name, f.verb, refused, error));
        }
    }
}
