# Launcher app

If you're working on `apps/launcher`, this page shows where code goes: the shell routes, each feature's state holds its own behaviour, and a state with background work reaches the rest of the app through a narrow host.

## Layout

```text
apps/launcher/src/
  app/        logging, fonts, dialog keys
  services/   game discovery, probing and launch; downloads; the library
  shell/      the Shell entity: startup, navigation, top bar, play bar, host.rs
  dialogs/    one file or folder per dialog
  features/   one folder per page: its state, its page and its actions
  ui/         shared widgets and theme tokens
  blocks/     the addon UI blocks: forms, tables, charts
```

## The shell routes

`Shell` (`shell/mod.rs`) is the window's one entity. It holds a plain field for each feature's state and renders the pages. Its own methods compose: Play reads about ten states and raises their dialogs, and a probe tick hands its result to the game, shader, database and notice states. A method that needs only one state belongs on that state.

## A state with background work

A state that starts background work implements `Hosted`, and `Shell` implements `Host` for it (`shell/host.rs`):

```rust
pub(crate) trait Hosted: Sized + 'static {
    type Needs<'a>;  // what it borrows from the shell beside itself
    type Change;     // what it reports for other features to follow
}

pub(crate) trait Host<T: Hosted>: Sized + 'static {
    fn split(&mut self) -> (&mut T, T::Needs<'_>);
    fn changed(&mut self, change: T::Change, cx: &mut Context<Self>);
}
```

Its async methods are generic over `H: Host<Self>`. They reach their own state and its `Needs` and nothing else, spawn in `Context<H>`, and write back through `host.split()` after the await. The state changes themselves are plain `&mut self` methods with unit tests.

The support page (`features/support/state.rs`) is the worked example:

```rust
impl Hosted for SupportPage {
    type Needs<'a> = SupportNeeds<'a>;  // { session: Option<&'a Session> }
    type Change = SupportChange;        // Faces(Vec<String>)
}

impl SupportPage {
    pub(super) fn save<H: Host<Self>>(host: &mut H, bio: String, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        if !page.begin(needs.signed_in()) {
            return;
        }
        let (api, mode) = (needs.api(), page.mode.clone());
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async move { api.support_update(Some(&bio), Some(&mode)) }).await;
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
}

impl Host<SupportPage> for Shell {
    fn split(&mut self) -> (&mut SupportPage, SupportNeeds<'_>) {
        (&mut self.support, SupportNeeds { session: self.account.session.as_ref() })
    }

    fn changed(&mut self, change: SupportChange, cx: &mut Context<Self>) {
        match change {
            SupportChange::Faces(faces) => self.load_faces(faces, cx),
        }
    }
}
```

A page calls it with the shell as the host: `SupportPage::save(this, typed.clone(), cx)`. The test drives the plain change:

```rust
#[test]
fn a_saved_bio_reloads_and_a_failed_one_says_why() {
    let mut page = SupportPage { busy: true, ..Default::default() };
    assert!(page.take_saved(Ok(())));
    page.busy = true;
    assert!(!page.take_saved(Err(error("too long"))));
    assert_eq!(page.notice, Some((false, "too long".to_owned())));
}
```

The same pattern runs the account (`shell/account.rs`), the crash watch (`dialogs/crash.rs`) and the time trial page (`features/time_trial/state.rs`). A state with nothing to report sets `type Change = Infallible`, and its `changed` is `match change {}`.

## When to use an entity instead

Make a part its own `Entity` when it renders itself and the shell only listens to it. The shaders page is one: `ShadersView` implements `Render` and `EventEmitter<Event>`, and `shell/startup.rs` subscribes to its events. Dialogs and the gpui-component inputs and sliders work the same way.

Keep a plain field with `Hosted` when the shell renders the state and code reads it while rendering, often from `&self` helpers with no `cx`. The account, the game copies and the launch are read that way in close to two hundred places.
