use crate::shell::notices::Notification;
use gpui_kit::*;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use swofty_core::online::server::sign_in::{self, Ban, Refreshed, Session, SignInError};
use swofty_i18n::{tr, trf};

use super::Shell;
use super::host::{Host, Hosted};

#[derive(Default)]
pub(crate) struct Account {
    pub(crate) session: Option<Session>,
    pub(crate) busy: bool,
    pub(crate) ban: Option<Ban>,
    pub(crate) toasts: Vec<Notification>,
    pub(crate) avatar: Option<Arc<Image>>,
    pub(crate) avatar_link: Option<String>,
}

impl Account {
    pub(crate) fn allowed(&self) -> bool {
        self.session.as_ref().is_some_and(|m| m.allowed)
    }

    pub(crate) fn display_name(&self) -> Option<String> {
        self.session.as_ref().map(|s| s.name.clone()).or_else(|| {
            cfg!(debug_assertions)
                .then(|| std::env::var("SWOFTY_FAKE_SIGNED_IN").ok())
                .flatten()
        })
    }

    /// Turns signing in or out away while the game runs, with a toast saying why.
    pub(crate) fn locked_while(&mut self, game_busy: bool) -> bool {
        match account_guard(game_busy) {
            Ok(()) => false,
            Err(why) => {
                self.toasts.push(Notification::warning(why));
                true
            }
        }
    }

    pub(crate) fn sign_in_failed(&mut self, e: impl std::fmt::Display) {
        self.toasts
            .push(Notification::error(trf!("Sign-in failed: {e}", e = e)));
    }

    /// Takes the result of a browser sign-in. True when a session started.
    pub(crate) fn finish_sign_in(&mut self, result: Result<Session, SignInError>, data: &Path) -> bool {
        self.busy = false;
        match result {
            Ok(session) => {
                self.toasts.push(Notification::success(trf!(
                    "Signed in as {name}",
                    name = session.name
                )));
                if session.allowed {
                    self.toasts.push(Notification::success(tr(
                        "Your account has been granted access to: Multiplayer",
                    )));
                    swofty_core::online::multiplayer::client::load_in_background(data, &session.token);
                }
                self.session = Some(session);
                true
            }
            Err(SignInError::Banned(ban)) => {
                self.ban = Some(ban);
                false
            }
            Err(e) => {
                self.sign_in_failed(e);
                false
            }
        }
    }

    /// Takes a refreshed session, or none when it has gone. True when one is still signed in.
    pub(crate) fn take_refresh(
        &mut self,
        refreshed: Option<Session>,
        was_allowed: bool,
        data: &Path,
    ) -> bool {
        match refreshed {
            Some(session) => {
                let gained = session.allowed && !was_allowed;
                if gained {
                    swofty_core::online::multiplayer::client::forget_failure();
                }
                if session.allowed {
                    swofty_core::online::multiplayer::client::load_in_background(data, &session.token);
                }
                self.session = Some(session);
                if gained {
                    self.toasts.push(Notification::success(tr(
                        "Your account has been granted access to: Multiplayer",
                    )));
                }
                true
            }
            None => {
                self.session = None;
                false
            }
        }
    }

    pub(crate) fn take_ban(&mut self, ban: Ban) {
        self.session = None;
        self.ban = Some(ban);
    }
}

/// What the account borrows from the shell: the data folder, and whether the game is busy.
pub(crate) struct AccountNeeds<'a> {
    pub(crate) data: &'a Path,
    pub(crate) game_busy: bool,
}

/// What the shell follows when the account changes.
pub(crate) enum AccountChange {
    /// Who is signed in changed, or their session was refreshed.
    Session,
    /// Signed out or banned, which also leaves the online session.
    Left,
}

impl Hosted for Account {
    type Needs<'a> = AccountNeeds<'a>;
    type Change = AccountChange;
}

impl Account {
    pub(crate) fn sign_in<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (account, needs) = host.split();
        if account.busy || account.session.is_some() {
            return;
        }
        if account.locked_while(needs.game_busy) {
            cx.notify();
            return;
        }
        let (pending, url) = match sign_in::begin() {
            Ok(v) => v,
            Err(e) => {
                account.sign_in_failed(e);
                cx.notify();
                return;
            }
        };
        account.busy = true;
        cx.open_url(&url);
        let dir = needs.data.to_path_buf();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { pending.wait(&dir) })
                .await;
            this.update(cx, |host, cx| {
                let (account, needs) = host.split();
                if account.finish_sign_in(result, needs.data) {
                    Self::sync_avatar(host, cx);
                    host.changed(AccountChange::Session, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn sign_out<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (account, needs) = host.split();
        if account.session.is_some() && account.locked_while(needs.game_busy) {
            cx.notify();
            return;
        }
        let Some(session) = account.session.take() else {
            return;
        };
        let dir = needs.data.to_path_buf();
        host.changed(AccountChange::Left, cx);
        let name = session.name.clone();
        cx.background_executor()
            .spawn(async move { sign_in::sign_out(&dir, &session) })
            .detach();
        host.split()
            .0
            .toasts
            .push(Notification::info(trf!("Signed out {name}", name = name)));
        cx.notify();
    }

    pub(crate) fn apply_check<H: Host<Self>>(
        host: &mut H,
        checked: Refreshed,
        was_allowed: bool,
        cx: &mut Context<H>,
    ) {
        match checked {
            Refreshed::Session(session) => Self::apply_refresh(host, Some(session), was_allowed, cx),
            Refreshed::Gone => Self::apply_refresh(host, None, was_allowed, cx),
            Refreshed::Banned(ban) => {
                host.split().0.take_ban(ban);
                host.changed(AccountChange::Left, cx);
                cx.notify();
            }
        }
    }

    fn apply_refresh<H: Host<Self>>(
        host: &mut H,
        refreshed: Option<Session>,
        was_allowed: bool,
        cx: &mut Context<H>,
    ) {
        let (account, needs) = host.split();
        if account.take_refresh(refreshed, was_allowed, needs.data) {
            Self::sync_avatar(host, cx);
        }
        host.changed(AccountChange::Session, cx);
        cx.notify();
    }

    /// Checks the session with the server every `every`, for as long as the host lives.
    pub(crate) fn keep_checked<H: Host<Self>>(every: Duration, cx: &mut Context<H>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(every).await;
                let current = match this.update(cx, |host, _| {
                    let (account, needs) = host.split();
                    account.session.clone().map(|s| (needs.data.to_path_buf(), s))
                }) {
                    Ok(c) => c,
                    Err(_) => break,
                };
                let Some((dir, session)) = current else {
                    continue;
                };
                let was_allowed = session.allowed;
                let refreshed = cx
                    .background_executor()
                    .spawn(async move { sign_in::refresh_checked(&dir, &session) })
                    .await;
                if this
                    .update(cx, |host, cx| Self::apply_check(host, refreshed, was_allowed, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    pub(crate) fn sync_avatar<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (account, needs) = host.split();
        let Some(link) = account.session.as_ref().and_then(|s| s.avatar.clone()) else {
            return;
        };
        if account.avatar_link.as_ref() == Some(&link) {
            return;
        }
        account.avatar_link = Some(link.clone());
        let dir = needs.data.to_path_buf();
        cx.spawn(async move |this, cx| {
            let image = cx
                .background_executor()
                .spawn(async move {
                    crate::services::avatar::fetch(&dir, &link)
                        .then(|| crate::services::avatar::load(&dir))
                        .flatten()
                })
                .await;
            this.update(cx, |host, cx| {
                if image.is_some() {
                    host.split().0.avatar = image;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Host<Account> for Shell {
    fn split(&mut self) -> (&mut Account, AccountNeeds<'_>) {
        let game_busy = self.game_busy();
        (
            &mut self.account,
            AccountNeeds {
                data: &self.store.root,
                game_busy,
            },
        )
    }

    fn changed(&mut self, change: AccountChange, cx: &mut Context<Self>) {
        crate::features::store::StorePage::sync(self, cx);
        if matches!(change, AccountChange::Left) {
            self.room_leave(cx);
        }
        self.follow_online_access(cx);
    }
}

pub(crate) fn account_guard(game_busy: bool) -> Result<(), &'static str> {
    match game_busy {
        true => Err(swofty_i18n::tr("Close the game to sign in or out")),
        false => Ok(()),
    }
}

#[cfg(test)]
mod account_tests {
    use super::{Account, account_guard};

    #[test]
    fn the_account_is_locked_only_while_the_game_runs() {
        assert!(account_guard(false).is_ok());
        assert_eq!(account_guard(true), Err("Close the game to sign in or out"));
    }

    #[test]
    fn signing_in_or_out_is_refused_with_a_toast_while_the_game_runs() {
        let mut account = Account::default();
        assert!(!account.locked_while(false));
        assert!(account.toasts.is_empty());
        assert!(account.locked_while(true));
        assert_eq!(account.toasts.len(), 1);
    }

    #[test]
    fn a_gone_session_signs_the_account_out() {
        let mut account = Account::default();
        assert!(!account.take_refresh(None, true, std::path::Path::new("unused")));
        assert!(account.session.is_none());
        assert!(account.toasts.is_empty());
    }
}
