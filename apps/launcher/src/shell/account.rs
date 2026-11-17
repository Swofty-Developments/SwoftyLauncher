use crate::shell::notices::Notification;
use gpui_kit::*;
use std::sync::Arc;
use swofty_i18n::{tr, trf};

use super::Shell;

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
}

impl Shell {
    pub(crate) fn mp_sign_in(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.account.busy || self.account.session.is_some() {
            return;
        }
        if let Err(why) = account_guard(self.game_busy()) {
            self.account.toasts.push(Notification::warning(why));
            cx.notify();
            return;
        }
        let (pending, url) = match swofty_core::server::sign_in::begin() {
            Ok(v) => v,
            Err(e) => {
                self.account
                    .toasts
                    .push(Notification::error(trf!("Sign-in failed: {e}", e = e)));
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
                s.account.busy = false;
                match result {
                    Ok(session) => {
                        s.account.toasts.push(Notification::success(trf!(
                            "Signed in as {name}",
                            name = session.name
                        )));
                        if session.allowed {
                            s.account.toasts.push(Notification::success(tr(
                                "Your account has been granted access to: Multiplayer",
                            )));
                        }
                        swofty_core::multiplayer::client::load_in_background(&s.store.root, &session.token);
                        s.account.session = Some(session);
                        s.sync_avatar(cx);
                        s.sync_support(cx);
                    }
                    Err(swofty_core::server::sign_in::SignInError::Banned(ban)) => s.account.ban = Some(ban),
                    Err(e) => s
                        .account
                        .toasts
                        .push(Notification::error(trf!("Sign-in failed: {e}", e = e))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn mp_sign_out(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.account.session.is_some()
            && let Err(why) = account_guard(self.game_busy())
        {
            self.account.toasts.push(Notification::warning(why));
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
                self.account.session = None;
                self.sync_support(cx);
                self.account.ban = Some(ban);
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
        match refreshed {
            Some(session) => {
                let gained = session.allowed && !was_allowed;
                swofty_core::multiplayer::client::load_in_background(&self.store.root, &session.token);
                self.account.session = Some(session);
                self.sync_avatar(cx);
                if gained {
                    self.account.toasts.push(Notification::success(tr(
                        "Your account has been granted access to: Multiplayer",
                    )));
                }
            }
            None => self.account.session = None,
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
    use super::account_guard;

    #[test]
    fn the_account_is_locked_only_while_the_game_runs() {
        assert!(account_guard(false).is_ok());
        assert_eq!(account_guard(true), Err("Close the game to sign in or out"));
    }
}
