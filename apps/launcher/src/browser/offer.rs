use std::collections::BTreeSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable};
use gpui_kit::*;
use swofty_core::catalog::Addon;
use swofty_core::catalog::requires::{self, Have, Plan};
use swofty_core::update::current_version;
use swofty_i18n::{tr, trf};

use super::*;
use crate::library::Source;
use crate::shell::{Page, Shell};

impl Shell {
    pub(super) fn offer(&self, a: &Addon) -> Offer {
        if let Some(job) = self.browser.working.get(&a.id) {
            return Offer::Working(job.progress());
        }
        if self
            .browser
            .queue
            .jobs()
            .iter()
            .any(|job| job.id == a.id && job.state == crate::download_queue::State::Queued)
        {
            return Offer::Queued;
        }
        if self.browser.failed.contains_key(&a.id) {
            return Offer::Failed;
        }
        let elsewhere = self
            .library
            .addons
            .iter()
            .any(|m| m.manifest.id == a.id && !m.in_profile);
        match (a.offered(&current_version()), self.installed_version(&a.id)) {
            (_, Some(_)) if elsewhere => Offer::Elsewhere,
            (Some(v), Some(m)) if v.version > m => Offer::Update(m, v.version.clone()),
            (_, Some(m)) => Offer::Installed(m),
            (Some(_), None) if self.needs_of(a).1.is_some() => Offer::Blocked,
            (Some(_), None) => Offer::Install,
            (None, None) => Offer::TooOld,
        }
    }

    /// What installing the version of `a` this launcher would get also
    /// installs, from the requirements the registry lists; none when it doesn't
    /// list them all.
    pub(super) fn requirements(&self, a: &Addon) -> Option<swofty_core::Result<Plan>> {
        let launcher = current_version();
        let v = a.offered(&launcher)?;
        // An unknown root requirement list can never produce a complete preview.
        v.requires.as_ref()?;
        let builtin: BTreeSet<&str> = self
            .library
            .addons
            .iter()
            .filter(|x| x.source == Source::Builtin)
            .map(|x| x.manifest.id.as_str())
            .collect();
        let have = Have::new(self.library.addons.iter().map(|x| &x.manifest), |id| {
            builtin.contains(id)
        });
        requires::preview(a, v, &self.browser.addons, &have, &launcher)
    }

    /// The other addons installing `a` brings, as `Name 1.2.0`, and why it
    /// can't be installed when the registry's requirements already say.
    pub(super) fn needs_of(&self, a: &Addon) -> (Vec<String>, Option<String>) {
        if let Some(job) = self.browser.working.get(&a.id) {
            return (job.also(), None);
        }
        if self.installed_version(&a.id).is_some() {
            return (Vec::new(), None);
        }
        if let Some(cached) = self.browser.needs_cache.borrow().get(&a.id) {
            return cached.clone();
        }
        let needs = match self.requirements(a) {
            Some(Ok(plan)) => (plan.also().iter().map(|s| s.label()).collect(), None),
            Some(Err(e)) => (Vec::new(), Some(e.to_string())),
            None => (Vec::new(), None),
        };
        self.browser
            .needs_cache
            .borrow_mut()
            .insert(a.id.clone(), needs.clone());
        needs
    }

    /// A version's requirements as `Requires Name >=1.0.0, other.id`.
    pub(super) fn requires_text(&self, reqs: &[swofty_core::manifest::Requirement]) -> String {
        let named: Vec<String> = reqs
            .iter()
            .map(|r| {
                let name = self
                    .browser
                    .find(&r.id)
                    .map_or(r.id.as_str(), |a| a.name.as_str());
                match r.version == semver::VersionReq::STAR {
                    true => name.to_owned(),
                    false => format!("{name} {}", r.version),
                }
            })
            .collect();
        trf!("Requires {names}", names = named.join(", "))
    }

    pub(crate) fn offer_button(&self, a: &Addon, id: String, cx: &mut Context<Self>) -> Button {
        let target = a.id.clone();
        let button = Button::new(SharedString::from(id)).small();
        let button = match self.offer(a) {
            Offer::Install => button.primary().icon(IconName::Download).label(tr("Install")),
            Offer::Update(from, to) => {
                return button
                    .outline()
                    .icon(IconName::CircleArrowUp)
                    .label(trf!(
                        "Installed v{from} · v{to} in the play bar",
                        from = from,
                        to = to
                    ))
                    .disabled(true);
            }
            Offer::Failed => {
                return button
                    .danger()
                    .icon(IconName::RotateCw)
                    .label(tr("Failed – Retry"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.retry_download(&target, cx);
                    }));
            }
            Offer::Installed(v) => {
                return button
                    .outline()
                    .icon(IconName::Check)
                    .label(trf!("Installed v{version}", version = v))
                    .disabled(true);
            }
            Offer::Elsewhere => {
                let label = trf!("Add to {profile}", profile = self.library.profile.name);
                return button
                    .primary()
                    .icon(IconName::Plus)
                    .label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.add_one_to_profile(&target, window, cx);
                    }));
            }
            Offer::TooOld => {
                return button
                    .outline()
                    .label(tr("Needs a newer launcher"))
                    .disabled(true);
            }
            Offer::Blocked => {
                return button
                    .outline()
                    .icon(IconName::TriangleAlert)
                    .label(tr("Missing requirements"))
                    .disabled(true);
            }
            Offer::Queued => {
                return button
                    .outline()
                    .label(tr("Queued · View downloads"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_page(Page::Downloads, cx);
                    }));
            }
            Offer::Working(progress) => {
                return button
                    .primary()
                    .loading(true)
                    .label(progress.short())
                    .disabled(true);
            }
        };
        button.on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.install_from_catalog(&target, cx);
        }))
    }
}
