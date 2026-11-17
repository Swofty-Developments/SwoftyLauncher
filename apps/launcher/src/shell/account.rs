use crate::shell::notices::Notification;
use gpui_kit::*;
use std::path::Path;
use std::sync::Arc;
use swofty_core::server::sign_in::{Ban, Session, SignInError};
use swofty_i18n::{tr, trf};

use super::Shell;

#[derive(Default)]
pub(crate) struct Account {
    pub(crate) session: Option<swofty_core::server::sign_in::Session>,
    pub(crate) busy: bool,
    pub(crate) ban: Option<swofty_core::server::sign_in::Ban>,
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
                }
                swofty_core::multiplayer::client::load_in_background(data, &session.token);
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
                swofty_core::multiplayer::client::load_in_background(data, &session.token);
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

impl Shell {
    pub(crate) fn mp_sign_in(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.account.busy || self.account.session.is_some() {
            return;
        }
        if self.account.locked_while(self.game_busy()) {
            cx.notify();
            return;
        }
        let (pending, url) = match swofty_core::server::sign_in::begin() {
            Ok(v) => v,
            Err(e) => {
                self.account.sign_in_failed(e);
                cx.notify();
                return;
            }
        };
        self.account.busy = true;
        cx.open_url(&url);
        let dir = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { pending.wait(&dir) })
                .await;
            this.update(cx, |s, cx| {
                if s.account.finish_sign_in(result, &s.store.root) {
                    s.sync_avatar(cx);
                    s.sync_support(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn mp_sign_out(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.account.session.is_some() && self.account.locked_while(self.game_busy()) {
            cx.notify();
            return;
        }
        let Some(session) = self.account.session.take() else {
            return;
        };
        self.sync_support(cx);
        self.mp_leave(cx);
        let name = session.name.clone();
        let dir = self.store.root.clone();
        cx.background_executor()
            .spawn(async move { swofty_core::server::sign_in::sign_out(&dir, &session) })
            .detach();
        self.account
            .toasts
            .push(Notification::info(trf!("Signed out {name}", name = name)));
        cx.notify();
    }

    pub(crate) fn mp_apply_check(
        &mut self,
        checked: swofty_core::server::sign_in::Refreshed,
        was_allowed: bool,
        cx: &mut Context<Self>,
    ) {
        use swofty_core::server::sign_in::Refreshed;
        match checked {
            Refreshed::Session(session) => self.mp_apply_refresh(Some(session), was_allowed, cx),
            Refreshed::Gone => self.mp_apply_refresh(None, was_allowed, cx),
            Refreshed::Banned(ban) => {
                self.mp_leave(cx);
                self.account.take_ban(ban);
                self.sync_support(cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn mp_apply_refresh(
        &mut self,
        refreshed: Option<swofty_core::server::sign_in::Session>,
        was_allowed: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .account
            .take_refresh(refreshed, was_allowed, &self.store.root)
        {
            self.sync_avatar(cx);
        }
        self.sync_support(cx);
        cx.notify();
    }
}

impl Shell {
    pub(crate) fn sync_avatar(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.account.session.as_ref().and_then(|s| s.avatar.clone()) else {
            return;
        };
        if self.account.avatar_link.as_ref() == Some(&link) {
            return;
        }
        self.account.avatar_link = Some(link.clone());
        let dir = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let image = cx
                .background_executor()
                .spawn(async move {
                    crate::services::avatar::fetch(&dir, &link)
                        .then(|| crate::services::avatar::load(&dir))
                        .flatten()
                })
                .await;
            this.update(cx, |s, cx| {
                if image.is_some() {
                    s.account.avatar = image;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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
