use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::online::server::api::{Api, ApiError};
use swofty_core::online::server::sign_in::Session;
use swofty_core::online::server::support::{
    BIO_MAX, Claimed, Kind, Me, Preview, Supporter, dollars_to_cents,
};
use swofty_i18n::tr;

use crate::features::time_trial::Load;
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted};

pub(super) struct Inputs {
    pub bio: Entity<InputState>,
    pub amount: Entity<InputState>,
    pub email: Entity<InputState>,
    pub payment: Entity<InputState>,
    _subs: Vec<Subscription>,
}

pub(crate) struct StorePage {
    pub supporters: Load<Vec<Supporter>>,
    pub previews: Vec<Preview>,
    pub me: Load<Me>,
    pub(super) inputs: Option<Inputs>,
    pub bio_synced: bool,
    pub mode: String,
    pub busy: bool,
    pub notice: Option<(bool, String)>,
    pub claimed: Option<Claimed>,
    pub claim_open: bool,
    pub listing_open: bool,
}

impl Default for StorePage {
    fn default() -> Self {
        Self {
            supporters: Load::Idle,
            previews: Vec::new(),
            me: Load::Idle,
            inputs: None,
            bio_synced: false,
            mode: "discord".to_owned(),
            busy: false,
            notice: None,
            claimed: None,
            claim_open: false,
            listing_open: false,
        }
    }
}

/// What the store page borrows from the shell: who is signed in.
pub(crate) struct StoreNeeds<'a> {
    pub(crate) session: Option<&'a Session>,
}

impl StoreNeeds<'_> {
    fn api(&self) -> Api {
        Api::new(self.session.map(|s| s.token.as_str()).unwrap_or_default())
    }

    fn signed_in(&self) -> bool {
        self.session.is_some()
    }
}

/// What the shell follows when the store page loads.
pub(crate) enum StoreChange {
    /// The supporters' avatars and the previews' pictures to fetch.
    Images(Vec<String>),
}

impl Hosted for StorePage {
    type Needs<'a> = StoreNeeds<'a>;
    type Change = StoreChange;
}

impl StorePage {
    pub(super) fn inputs<H: Host<Self>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> (
        Entity<InputState>,
        Entity<InputState>,
        Entity<InputState>,
        Entity<InputState>,
    ) {
        if host.split().0.inputs.is_none() {
            let make = |hint: &'static str, window: &mut Window, cx: &mut Context<H>| {
                cx.new(|cx| InputState::new(window, cx).placeholder(tr(hint)))
            };
            let bio = make("One sentence about you (shown next to your name)", window, cx);
            let amount = make("Amount, e.g. 20", window, cx);
            let email = make("Email used on Stripe", window, cx);
            let payment = make("Invoice or payment ID", window, cx);
            let subs = [&bio, &amount, &email, &payment]
                .into_iter()
                .map(|state| {
                    cx.subscribe(state, |_, _, ev: &InputEvent, cx| {
                        if matches!(ev, InputEvent::Change) {
                            cx.notify();
                        }
                    })
                })
                .collect();
            host.split().0.inputs = Some(Inputs {
                bio,
                amount,
                email,
                payment,
                _subs: subs,
            });
        }
        let i = host.split().0.inputs.as_ref().expect("made above");
        (
            i.bio.clone(),
            i.amount.clone(),
            i.email.clone(),
            i.payment.clone(),
        )
    }

    /// Loads the page when it opens, unless what it shows is already loaded or loading.
    pub(crate) fn open<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        if page.needs_load(needs.signed_in()) {
            Self::reload(host, cx);
        }
    }

    /// Fetches the signed-in user's support once per sign-in, which decides their ads.
    pub(crate) fn sync<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        if page.follow_sign_in(needs.signed_in()) {
            Self::fetch_me(host, cx);
        }
    }

    fn fetch_me<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        let api = needs.api();
        page.me = Load::Loading;
        cx.spawn(async move |this, cx| {
            let me = cx
                .background_executor()
                .spawn(async move { api.support_me() })
                .await;
            this.update(cx, |host, cx| {
                let (page, needs) = host.split();
                if needs.signed_in() {
                    page.me = load(me);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn reload<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        let api = needs.api();
        let signed_in = needs.signed_in();
        page.reloading(signed_in);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let (supporters, previews, me) = cx
                .background_executor()
                .spawn(async move {
                    (
                        api.support_supporters(),
                        api.support_previews(),
                        signed_in.then(|| api.support_me()),
                    )
                })
                .await;
            this.update(cx, |host, cx| {
                let images = host.split().0.take_reload(supporters, previews, me);
                host.changed(StoreChange::Images(images), cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn checkout<H: Host<Self>>(
        host: &mut H,
        kind: Kind,
        amount: Option<String>,
        cx: &mut Context<H>,
    ) {
        let (page, needs) = host.split();
        let Ok(cents) = checkout_cents(kind, amount.as_deref()) else {
            page.notice = Some((false, tr("Enter an amount in dollars, like 20.").to_owned()));
            cx.notify();
            return;
        };
        if page.busy || (kind != Kind::OneTime && !needs.signed_in()) {
            return;
        }
        let api = needs.api();
        page.busy = true;
        page.notice = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let url = cx
                .background_executor()
                .spawn(async move { api.support_checkout(kind, cents) })
                .await;
            this.update(cx, |host, cx| {
                if let Some(url) = host.split().0.take_checkout(url) {
                    cx.open_url(&url);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn claim<H: Host<Self>>(host: &mut H, email: String, payment: String, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        if !page.begin(needs.signed_in()) {
            return;
        }
        let api = needs.api();
        page.claimed = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { api.support_claim(email.trim(), payment.trim()) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.take_claim(got) {
                    Self::reload(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn save<H: Host<Self>>(host: &mut H, bio: String, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        if !page.begin(needs.signed_in()) {
            return;
        }
        let api = needs.api();
        let bio: String = bio.trim().chars().take(BIO_MAX).collect();
        let mode = page.mode.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { api.support_update(Some(&bio), Some(&mode)) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.take_saved(got) {
                    Self::reload(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn needs_load(&self, signed_in: bool) -> bool {
        matches!(self.supporters, Load::Idle | Load::Failed(_))
            || (signed_in && matches!(self.me, Load::Idle | Load::Failed(_)))
    }

    /// Forgets the support status of someone who signed out. True when a signed-in user's status
    /// still has to be fetched.
    fn follow_sign_in(&mut self, signed_in: bool) -> bool {
        if !signed_in {
            self.me = Load::Idle;
            return false;
        }
        matches!(self.me, Load::Idle | Load::Failed(_))
    }

    fn reloading(&mut self, signed_in: bool) {
        self.supporters = Load::Loading;
        self.me = if signed_in { Load::Loading } else { Load::Idle };
    }

    /// Takes a reload's results, and gives back the supporters' avatar links and the previews' pictures.
    /// Previews that fail to load, as on servers from before them, are left out.
    fn take_reload(
        &mut self,
        supporters: Result<Vec<Supporter>, ApiError>,
        previews: Result<Vec<Preview>, ApiError>,
        me: Option<Result<Me, ApiError>>,
    ) -> Vec<String> {
        self.previews = previews.unwrap_or_default();
        let supporters = supporters.map(|mut all| {
            all.sort_by_key(|x| !x.plus());
            all
        });
        let images = supporters
            .iter()
            .flatten()
            .filter_map(|x| x.display.avatar_url.clone())
            .chain(self.previews.iter().filter_map(|p| p.icon.clone()))
            .collect();
        self.supporters = load(supporters);
        self.me = me.map(load).unwrap_or(Load::Idle);
        if let Load::Ready(me) = &self.me {
            self.mode = me.display_mode.clone().unwrap_or_else(|| "discord".into());
        }
        self.bio_synced = false;
        images
    }

    /// Starts a change that needs a signed-in user, unless one is already running.
    fn begin(&mut self, signed_in: bool) -> bool {
        if !signed_in || self.busy {
            return false;
        }
        self.busy = true;
        self.notice = None;
        true
    }

    /// Takes the checkout link to open, or says why there is none.
    fn take_checkout(&mut self, url: Result<String, ApiError>) -> Option<String> {
        self.busy = false;
        url.map_err(|e| self.notice = Some((false, e.message))).ok()
    }

    /// Takes a claim's result. True when the page has to load again to show it.
    fn take_claim(&mut self, got: Result<Claimed, ApiError>) -> bool {
        self.busy = false;
        match got {
            Ok(claimed) => {
                self.claimed = Some(claimed);
                true
            }
            Err(e) => {
                self.notice = Some((false, e.message));
                false
            }
        }
    }

    /// Takes a save's result. True when the page has to load again to show it.
    fn take_saved(&mut self, got: Result<(), ApiError>) -> bool {
        self.busy = false;
        self.notice = Some(match got {
            Ok(()) => (true, tr("Saved.").to_owned()),
            Err(e) => (false, e.message),
        });
        self.notice.as_ref().is_some_and(|(ok, _)| *ok)
    }
}

impl Host<StorePage> for Shell {
    fn split(&mut self) -> (&mut StorePage, StoreNeeds<'_>) {
        (
            &mut self.store_page,
            StoreNeeds {
                session: self.account.session.as_ref(),
            },
        )
    }

    fn changed(&mut self, change: StoreChange, cx: &mut Context<Self>) {
        match change {
            StoreChange::Images(images) => {
                crate::features::multiplayer::Multiplayer::load_faces(self, images, cx)
            }
        }
    }
}

/// The cents a checkout asks for: a one-time gift needs an amount in dollars, a subscription none.
fn checkout_cents(kind: Kind, amount: Option<&str>) -> Result<Option<u64>, ()> {
    match kind {
        Kind::OneTime => amount.and_then(dollars_to_cents).map(Some).ok_or(()),
        _ => Ok(None),
    }
}

fn load<T>(got: Result<T, ApiError>) -> Load<T> {
    match got {
        Ok(v) => Load::Ready(v),
        Err(e) => Load::Failed(e.message),
    }
}

#[cfg(test)]
mod tests {
    use swofty_core::online::server::api::ApiError;
    use swofty_core::online::server::support::{Kind, Preview, Supporter};

    use super::{Load, StorePage, checkout_cents};

    fn error(message: &str) -> ApiError {
        ApiError {
            status: 500,
            message: message.to_owned(),
            retry_after: None,
        }
    }

    fn supporter(avatar: Option<&str>) -> Supporter {
        serde_json::from_value(serde_json::json!({
            "tier": "plus",
            "display": { "avatar_url": avatar },
        }))
        .unwrap()
    }

    fn member(tier: &str, name: &str) -> Supporter {
        serde_json::from_value(serde_json::json!({
            "tier": tier,
            "display": { "name": name },
        }))
        .unwrap()
    }

    fn preview(id: &str, image: Option<&str>) -> Preview {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "title": "F1 2011",
            "summary": "Early beta testing",
            "highlights": ["KERS"],
            "image": image,
            "icon": image.map(|x| x.replace(".png", "-icon.png")),
            "addons": [],
            "posted": 1_763_480_000u64,
            "link": null,
        }))
        .unwrap()
    }

    #[test]
    fn a_one_time_gift_needs_an_amount_and_a_subscription_none() {
        assert!(checkout_cents(Kind::OneTime, None).is_err());
        assert!(checkout_cents(Kind::OneTime, Some("lots")).is_err());
        assert_eq!(checkout_cents(Kind::OneTime, Some("20")), Ok(Some(2000)));
        assert_eq!(checkout_cents(Kind::Supporter, Some("lots")), Ok(None));
    }

    #[test]
    fn the_page_loads_until_its_supporters_and_the_users_status_are_in() {
        let mut page = StorePage::default();
        assert!(page.needs_load(false));
        page.reloading(false);
        assert!(!page.needs_load(false));
        assert!(matches!(page.me, Load::Idle));
        assert!(page.needs_load(true));
        page.reloading(true);
        assert!(!page.needs_load(true));
    }

    #[test]
    fn signing_out_forgets_the_status_and_signing_in_fetches_it_once() {
        let mut page = StorePage::default();
        assert!(page.follow_sign_in(true));
        page.me = Load::Loading;
        assert!(!page.follow_sign_in(true));
        assert!(!page.follow_sign_in(false));
        assert!(matches!(page.me, Load::Idle));
        page.me = Load::Failed("down".to_owned());
        assert!(page.follow_sign_in(true));
    }

    #[test]
    fn a_reload_shows_its_results_and_hands_back_the_pictures_to_fetch() {
        let mut page = StorePage {
            bio_synced: true,
            ..Default::default()
        };
        let images = page.take_reload(
            Ok(vec![supporter(Some("https://a/1.png")), supporter(None)]),
            Ok(vec![
                preview("swofty-2011", Some("https://a/2011.png")),
                preview("bare", None),
            ]),
            None,
        );
        assert_eq!(images, ["https://a/1.png", "https://a/2011-icon.png"]);
        assert!(matches!(&page.supporters, Load::Ready(s) if s.len() == 2));
        assert_eq!(page.previews.len(), 2);
        assert!(matches!(page.me, Load::Idle));
        assert!(!page.bio_synced);
        page.take_reload(Err(error("supporters down")), Ok(Vec::new()), None);
        assert!(matches!(&page.supporters, Load::Failed(m) if m == "supporters down"));
    }

    #[test]
    fn supporter_plus_members_are_listed_first_in_the_order_the_server_sent() {
        let mut page = StorePage::default();
        page.take_reload(
            Ok(vec![
                member("supporter", "Ann"),
                member("supporter_plus", "Bo"),
                member("supporter", "Cy"),
                member("supporter_plus", "Di"),
            ]),
            Ok(Vec::new()),
            None,
        );
        let Load::Ready(all) = &page.supporters else {
            panic!("the supporters did not load");
        };
        let names: Vec<_> = all.iter().filter_map(|x| x.display.name.as_deref()).collect();
        assert_eq!(names, ["Bo", "Di", "Ann", "Cy"]);
    }

    #[test]
    fn previews_show_what_the_server_sent_and_hide_when_it_has_none_or_fails() {
        let mut page = StorePage::default();
        page.take_reload(Ok(Vec::new()), Ok(vec![preview("swofty-2011", None)]), None);
        assert_eq!(page.previews[0].id, "swofty-2011");
        let not_found = ApiError {
            status: 404,
            ..error("not found")
        };
        page.take_reload(Ok(Vec::new()), Err(not_found), None);
        assert!(page.previews.is_empty());
        page.take_reload(Ok(Vec::new()), Ok(vec![preview("swofty-2011", None)]), None);
        page.take_reload(Ok(Vec::new()), Err(error("down")), None);
        assert!(page.previews.is_empty());
        page.take_reload(Ok(Vec::new()), Ok(Vec::new()), None);
        assert!(page.previews.is_empty());
    }

    #[test]
    fn a_change_waits_for_sign_in_and_for_the_last_one_to_finish() {
        let mut page = StorePage::default();
        assert!(!page.begin(false));
        page.notice = Some((false, "old".to_owned()));
        assert!(page.begin(true));
        assert!(page.busy && page.notice.is_none());
        assert!(!page.begin(true));
    }

    #[test]
    fn a_saved_bio_reloads_and_a_failed_one_says_why() {
        let mut page = StorePage {
            busy: true,
            ..Default::default()
        };
        assert!(page.take_saved(Ok(())));
        assert!(!page.busy);
        page.busy = true;
        assert!(!page.take_saved(Err(error("too long"))));
        assert_eq!(page.notice, Some((false, "too long".to_owned())));
        page.busy = true;
        assert_eq!(page.take_checkout(Err(error("stripe down"))), None);
        assert!(!page.busy);
        assert_eq!(
            page.take_checkout(Ok("https://pay".to_owned())),
            Some("https://pay".to_owned())
        );
    }
}
