use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui_kit::*;
use swofty_core::catalog::publishers::{self, Creator};
use swofty_core::catalog::stars::{self};
use swofty_core::catalog::{self, Addon, Trouble, Update};
use swofty_core::updates::release::current_version;
use swofty_i18n::{tr, trf, trn};

use super::authors::View;
use super::*;
use crate::services::library::{Library, Source};
use crate::shell::Shell;
use crate::shell::host::Host;

pub(super) struct Fetched {
    pub(super) addons: Option<Vec<Addon>>,
    pub(super) icons: Vec<(String, Arc<Image>)>,
    pub(super) error: Option<Trouble>,
    pub(super) creators: std::result::Result<Vec<Creator>, Trouble>,
    pub(super) avatars: Vec<(String, Arc<Image>)>,
}

pub(super) fn images<'a>(
    dir: &Path,
    urls: impl IntoIterator<Item = &'a String>,
    known: &BTreeSet<String>,
) -> Vec<(String, Arc<Image>)> {
    let mut wanted: Vec<&String> = urls
        .into_iter()
        .filter(|url| web_url(url) && !known.contains(*url))
        .collect();
    wanted.sort();
    wanted.dedup();
    wanted
        .into_iter()
        .filter_map(|url| crate::features::news::image(dir, url).map(|i| (url.clone(), i)))
        .collect()
}

pub(super) fn avatar_dir(root: &Path) -> std::path::PathBuf {
    root.join(catalog::CACHE_DIR).join(AVATARS_DIR)
}

pub(super) fn fetch(root: &Path, shown: Vec<Addon>, known: BTreeSet<String>) -> Fetched {
    let (addons, error) = match catalog::fetch(root) {
        Ok(addons) => (Some(addons), None),
        Err(e) => (None, Some(e)),
    };
    let creators = publishers::fetch(root);
    let listed = addons.as_ref().unwrap_or(&shown);
    let icons = images(
        &root.join(catalog::CACHE_DIR).join(ICONS_DIR),
        listed.iter().filter_map(|a| a.icon.as_ref()),
        &known,
    );
    let faces = listed
        .iter()
        .flat_map(|a| a.owner().into_iter().chain(&a.publishers))
        .filter_map(|p| p.avatar.as_ref())
        .chain(creators.iter().flatten().filter_map(|c| c.avatar.as_ref()));
    let avatars = images(&avatar_dir(root), faces, &known);
    Fetched {
        addons,
        icons,
        error,
        creators,
        avatars,
    }
}

impl Browser {
    pub(crate) fn watch<H: Host<Self>>(cx: &mut Context<H>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let ticked = this.update(cx, |host, cx| {
                    let (b, needs) = host.split();
                    if needs.locked {
                        return;
                    }
                    if b.stale() {
                        Self::fetch_catalog(host, cx);
                    } else {
                        Self::sync_stars(host, cx);
                    }
                    cx.notify();
                });
                if ticked.is_err() {
                    break;
                }
            }
        })
    }

    pub(super) fn record(
        &mut self,
        root: &Path,
        op: Op,
        ok: bool,
        title: String,
        detail: String,
        took: Duration,
    ) {
        let mut line = format!(
            "{} {title} ({})",
            if ok { "ok" } else { "failed" },
            took_text(took)
        );
        if !detail.is_empty() {
            line.push_str(&format!(": {detail}"));
        }
        catalog::log(root, &line);
        self.feed.note(Activity {
            at: SystemTime::now(),
            op,
            ok,
            title,
            detail,
            took,
        });
    }

    pub(crate) fn fetch_catalog<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (b, needs) = host.split();
        if !b.feed.begin_fetch() {
            return;
        }
        let root = needs.store.root.clone();
        let shown = b.addons.clone();
        let known = b.icons.keys().chain(b.avatars.keys()).cloned().collect();
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move { fetch(&root, shown, known) })
                .await;
            this.update(cx, |host, cx| {
                let (b, needs) = host.split();
                b.take_fetched(&needs.store.root, fetched, started.elapsed());
                b.sync_updates(needs.library);
                Self::sync_stars(host, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn take_fetched(&mut self, root: &Path, fetched: Fetched, took: Duration) {
        let from = host(&catalog::base_url()).to_owned();
        match (&fetched.error, &fetched.addons) {
            (Some(e), _) => self.record(
                root,
                Op::Catalog,
                false,
                trf!("Couldn't fetch the catalog from {host}", host = from),
                e.to_string(),
                took,
            ),
            (None, Some(addons)) => self.record(
                root,
                Op::Catalog,
                true,
                tr("Fetched the catalog").to_owned(),
                trf!(
                    "{addons} from {host}",
                    addons = trn(addons.len(), "{count} addon", "{count} addons"),
                    host = from
                ),
                took,
            ),
            (None, None) => {}
        }
        match &fetched.creators {
            Ok(creators) => catalog::log(
                root,
                &format!("ok fetched {} publishers from {from}", creators.len()),
            ),
            Err(e) => catalog::log(root, &format!("failed to fetch the publishers: {e}")),
        }
        if let Ok(creators) = fetched.creators {
            self.creators = creators;
        }
        self.avatars.extend(fetched.avatars);
        self.feed.fetched(fetched.error, fetched.addons.is_some());
        if let Some(addons) = fetched.addons {
            self.addons = addons;
        }
        self.icons.extend(fetched.icons);
    }

    pub(crate) fn sync_stars<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (b, _) = host.split();
        if !b.feed.begin_sync() {
            return;
        }
        let sent = b.stars.outgoing();
        let client = b.stars.client.clone();
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let job = sent.clone();
            let result = cx
                .background_executor()
                .spawn(async move { stars::sync(&client, &job) })
                .await;
            this.update(cx, |host, cx| {
                let (b, needs) = host.split();
                if b.take_synced(&needs.store.root, &sent, result, started.elapsed()) {
                    Self::sync_stars(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn take_synced(
        &mut self,
        root: &Path,
        sent: &[stars::Change],
        result: std::result::Result<stars::Reply, Trouble>,
        took: Duration,
    ) -> bool {
        match result {
            Ok(reply) => {
                let applied = reply.applied;
                self.stars.synced(sent, reply, catalog::iso_now());
                self.catalog_revision = self.catalog_revision.wrapping_add(1);
                let detail = trf!(
                    "{sent}, {applied} applied · {starred} starred · {user}",
                    sent = trn(sent.len(), "{count} change sent", "{count} changes sent"),
                    applied = applied,
                    starred = self.stars.starred.len(),
                    user = self.stars.user.as_deref().unwrap_or(tr("no user yet"))
                );
                self.record(root, Op::Stars, true, tr("Synced stars").to_owned(), detail, took);
            }
            Err(e) => {
                self.stars.failed(e.to_string());
                let waiting = trn(
                    self.stars.pending.len(),
                    "{count} change waiting",
                    "{count} changes waiting",
                );
                self.record(
                    root,
                    Op::Stars,
                    false,
                    tr("Couldn't sync stars").to_owned(),
                    trf!("{error} · {waiting}", error = e, waiting = waiting),
                    took,
                );
            }
        }
        if let Err(e) = self.stars.save(root) {
            eprintln!("swofty-launcher: could not save the stars: {e}");
        }
        self.feed.synced()
    }

    pub(super) fn toggle_star<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) {
        let (b, needs) = host.split();
        let ticket = b.star(&needs.store.root, id);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STAR_DELAY).await;
            this.update(cx, |host, cx| {
                let settled = host.split().0.feed.settled(ticket);
                if settled {
                    Self::sync_stars(host, cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn star(&mut self, root: &Path, id: &str) -> u64 {
        let listed = self.find(id).map_or(0, |a| a.stars);
        let on = self.stars.toggle(id, listed, catalog::iso_now());
        if let Err(e) = self.stars.save(root) {
            eprintln!("swofty-launcher: could not save the stars: {e}");
        }
        catalog::log(
            root,
            &format!(
                "{} {id}, {} waiting",
                if on { "starred" } else { "unstarred" },
                self.stars.pending.len()
            ),
        );
        self.catalog_revision = self.catalog_revision.wrapping_add(1);
        self.feed.next_star()
    }

    pub(crate) fn sync_updates(&mut self, library: &Library) {
        // Catalog refreshes and library reloads both change cached filters and dependency previews.
        self.catalog_revision = self.catalog_revision.wrapping_add(1);
        self.needs_cache.borrow_mut().clear();
        let installed = library
            .addons
            .iter()
            .filter(|a| !matches!(a.source, Source::Builtin | Source::Importer))
            .map(|a| &a.manifest);
        self.updates = catalog::updates(installed, &self.addons, &current_version());
    }

    pub(super) fn tick<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (b, _) = host.split();
        if b.ticking {
            return;
        }
        b.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let done = this
                    .update(cx, |host, cx| {
                        let b = host.split().0;
                        let done = b.working.is_empty();
                        if done {
                            b.ticking = false;
                        }
                        cx.notify();
                        done
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn fetch_details<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) {
        let (b, _) = host.split();
        if !b.loading.insert(id.to_owned()) {
            return;
        }
        let id = id.to_owned();
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let job = id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { catalog::fetch_one(&job) })
                .await;
            this.update(cx, |host, cx| {
                let (b, needs) = host.split();
                b.take_details(&needs.store.root, id.clone(), result, started.elapsed());
                if let Some(a) = host.split().0.details.get(&id).cloned() {
                    Browser::load_media(host, &a, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn take_details(
        &mut self,
        root: &Path,
        id: String,
        result: std::result::Result<Addon, Trouble>,
        took: Duration,
    ) {
        self.loading.remove(&id);
        match result {
            Ok(a) => {
                let detail = trf!(
                    "{versions} · {trend}",
                    versions = trn(a.versions.len(), "{count} version", "{count} versions"),
                    trend = if a.trend.is_some() {
                        tr("30-day trend")
                    } else {
                        tr("no trend")
                    }
                );
                self.record(
                    root,
                    Op::Details,
                    true,
                    trf!("Fetched {name}", name = a.name),
                    detail,
                    took,
                );
                self.details.insert(id, a);
            }
            Err(e) => self.record(
                root,
                Op::Details,
                false,
                trf!("Couldn't fetch {id}", id = id),
                e.to_string(),
                took,
            ),
        }
    }
}

impl Shell {
    pub(crate) fn catalog_update(&self, id: &str) -> Option<&Update> {
        self.browser.updates.iter().find(|u| u.id == id)
    }

    pub(super) fn installed_version(&self, id: &str) -> Option<semver::Version> {
        installed_version(&self.library, id)
    }

    pub(super) fn open_details(&mut self, id: &str, cx: &mut Context<Self>) {
        self.browse(View::Addon(id.to_owned()), cx);
    }
}
