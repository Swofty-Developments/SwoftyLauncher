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

The store page (`features/store/state.rs`) is the worked example:

```rust
impl Hosted for StorePage {
    type Needs<'a> = StoreNeeds<'a>;  // { session: Option<&'a Session> }
    type Change = StoreChange;        // Images(Vec<String>)
}

impl StorePage {
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

impl Host<StorePage> for Shell {
    fn split(&mut self) -> (&mut StorePage, StoreNeeds<'_>) {
        (&mut self.store_page, StoreNeeds { session: self.account.session.as_ref() })
    }

    fn changed(&mut self, change: StoreChange, cx: &mut Context<Self>) {
        match change {
            StoreChange::Images(images) => Multiplayer::load_faces(self, images, cx),
        }
    }
}
```

A page calls it with the shell as the host: `StorePage::save(this, typed.clone(), cx)`. The test drives the plain change:

```rust
#[test]
fn a_saved_bio_reloads_and_a_failed_one_says_why() {
    let mut page = StorePage { busy: true, ..Default::default() };
    assert!(page.take_saved(Ok(())));
    page.busy = true;
    assert!(!page.take_saved(Err(error("too long"))));
    assert_eq!(page.notice, Some((false, "too long".to_owned())));
}
```

A state with nothing to report sets `type Change = Infallible`, and its `Host` impl writes `changed` with the macro from `shell/host.rs`:

```rust
impl Host<News> for Shell {
    fn split(&mut self) -> (&mut News, NewsNeeds<'_>) {
        (&mut self.news, NewsNeeds { root: &self.store.root })
    }

    reports_nothing!();
}
```

A state whose constructor needs a window keeps the fields its tests drive in a plain `Default` struct inside it, and the async method calls that struct's methods:

```rust
#[derive(Default)]
pub(crate) struct Autosave { ticket: u64, unwritten: bool, writing: bool, newer: bool, error: Option<String> }

impl Db {
    pub(crate) fn flush<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        if !db.autosave.begin() {
            return;
        }
        let (store, draft, file) = (needs.store.clone(), db.draft.clone(), db.file.clone());
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { lock(&file).write(&store, &draft) }).await;
            this.update(cx, |host, cx| {
                if host.split().0.autosave.written(result) {
                    Self::persist(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
```

These states run on the pattern:

| State | Background work | `Host` impl | Plain part with tests |
|---|---|---|---|
| `Account` | sign in and out, session checks, avatar | `shell/account.rs` | `Account` |
| `Crashes` | the crash watch | `shell/probe.rs` | `Crashes` |
| `StorePage` | supporters, previews, checkout, claim, save | `features/store/state.rs` | `StorePage` |
| `TtPage` | barred list, catalogue, runs, moderation, searches | `features/time_trial/state.rs` | `TtPage` |
| `Browser` | catalog fetch, star sync, addon details, publisher profiles, the install queue | `features/browser/state.rs` | `Feed`, `riders`, `installed_version` |
| `Db` | draft autosave, "Also changed by", game database, module views, search, unnamed keys, importing a file, sharing the addon, exporting game files, saving the draft; `DbChange` hands the shell a saved, new or imported addon to turn on and show, and `DbPrompt` hands it leaving the draft | `features/db/state.rs`, `Db::import` in `features/db/import.rs`, `Db::export_addon`, `Db::export_game_files_to`, `Db::save_bound` and `Db::save_new` in `features/db/save/` | `Autosave`, `DraftBook` (the draft and the DB Editor addons, `features/db/draft.rs`) |
| `AddonsPage` | the conflict check | `features/addons/conflicts.rs` | `Conflicts` |
| `Multiplayer` (the Online page) | game status, room list, addon digest, avatars, opening and joining a room, the page's tick, starting the room's game through Play, the room's game watch, closing the game for a room | `features/multiplayer/state.rs`; `Multiplayer::tick`, `launch` and `close_game` in `features/multiplayer/actions.rs` | `Lobby`, `starting`, `RoomGame` (`features/multiplayer/room_game.rs`) |
| `Online` (the time trial boards) | overview, seasons, boards, profiles, rankings, avatars and art, adding a lap's replay to the game | `features/online/mod.rs` | `Online` |
| `Updates` | the update check, installing a missing managed addon, running an update plan, its progress and the restart | `features/update.rs` | `Updates`, `holds_updates` |
| `SavesPage` | building the careers view and the saves banner | `features/career_saves/refresh.rs` | `SavesPage` |
| `Settings` | measuring the data folder | `features/settings/usage.rs` | `Settings` |
| `News` | checking the news feed | `features/news/state.rs` | `News` |
| `AddonViews` | building addon section views, refreshing stale ones, ticking the open section | `features/sections/mod.rs` | `AddonViews` |
| `Launching` | Play: the press, the checks, their outcome, the launch | `shell/play.rs` | `Launching`, `Press`, `Step` |

A page's toasts go into a `toasts` list on its state, and `shell/render.rs` shows them. A state reports what other features follow through its `Change`: the browser reports `Installed`, and the shell reads the library again, and `TurnOn`, and the shell adds the "Turn on" action to the notice.

Tests that need installed addons and an active profile read a temp data folder with `services/test_library.rs`:

```rust
let library = TestLibrary::new("tt-barred")
    .addon("t.old", "1.0.0")
    .profile("Main", &["t.old"]);
tt.barred = vec![barred("t.old", Some("2.0.0"))];
assert_eq!(tt.barred_in(&library.read()).len(), 1);
```

## What stays on the shell

| On `Shell` | Files | What it needs |
|---|---|---|
| Rendering every page, bar and dialog | `render.rs`, `features/*/page.rs`, `shell/play_bar/`, `shell/topbar/` | Pages read several states. gpui listeners are `Context<Shell>` closures. |
| Play's prompts, starting an update from the play bar | `shell/play_dialogs.rs`, `shell/launch_alerts.rs`, `dialogs/`, `Shell::play` in `shell/play.rs`, `start_updates` in `features/update.rs` | The window. `Shell::play` takes the room launch a prompt held off `Multiplayer` and hands it to `Launching::play_for`. |
| Reading the library again, profiles, turning addons on and off | `shell/reload.rs`, `shell/profile_panel.rs`, `shell/profile_edit.rs`, `features/addons/` | Every feature follows the library. |
| Startup | `shell/startup.rs` | Builds every state and starts each watch and poll loop. |
| The probe tick | `shell/probe.rs` | Hands its result to the game, shader, database and notice states. |
| The DB Editor's dialogs, turning a new or imported addon on, after a save | `features/db/save/`, `Shell::db_addon_imported` in `features/db/import.rs` | The save form, the Save / Discard dialog and the game files choice are `Shell` dialogs. A new addon and an imported one are turned on in the profile. After a save the library is read again and Addons shows the addon, or the draft moves on to what the Save / Discard dialog was leaving for. |
| Window flows | `dialogs/data_move.rs`, `dialogs/found_data.rs`, `dialogs/setup.rs`, `dialogs/send_logs.rs`, `features/onboarding/`, `features/career_saves/actions.rs`, clearing logs and the cache in `features/settings/usage.rs`, a section's startup, sends and intake in `features/sections/`, `features/importer.rs` | A dialog open in the window. |
| Tasks tied to rendering | `features/tools.rs`, `features/ads/` | The Tools row's glide and settle, the ad video's skip timer and opening an ad slot from its canvas. |
| The shaders page | `features/shaders/` | Its own entity, `ShadersView`. |

## A host that opens prompts

Play stops on a prompt: the setup dialog, the offline terms, the missing rows, a notice. A state whose work opens prompts names them as its `Prompt` type, and the shell opens them through `PromptHost` in `shell/host.rs`:

```rust
pub(crate) trait Prompted: Hosted {
    type Prompt;
}

pub(crate) trait PromptHost<T: Prompted>: Host<T> {
    fn prompt(&mut self, prompt: T::Prompt, window: &mut Window, cx: &mut Context<Self>);
}

impl Prompted for Launching {
    type Prompt = Prompt;
}

pub(crate) fn play_for<H: PromptHost<Launching>>(host: &mut H, /* … */) -> bool
```

```rust
let step = Step::of(stale, checked.gate, checked.verdict);
// …
if let Some(played) = step.played() {
    if let Some(prompt) = prompt {
        host.prompt(prompt, window, cx);
    }
    return played;
}
```

The DB Editor's `DbPrompt` works the same way: a draft that has to be left hands the shell the Save / Discard dialog. What needs no dialog goes through `changed` instead, and a `changed` that has a notice to show posts it with `notices::post`, which needs no window:

```rust
DbChange::Imported(done) => self.db_addon_imported(done, cx),

notices::post(Notification::success(body).title(title), cx);
```

`Press::of` decides what a press comes to before the checks run, and `Step::of` turns the checks' gate and verdict into the one thing Play does next. The tests in `shell/play.rs` pin each result of both.

## When to use an entity instead

Make a part its own `Entity` when it renders itself and the shell only listens to it. The shaders page is one: `ShadersView` implements `Render` and `EventEmitter<Event>`, and `shell/startup.rs` subscribes to its events. Dialogs and the gpui-component inputs and sliders work the same way.

Keep a plain field with `Hosted` when the shell renders the state and code reads it while rendering, often from `&self` helpers with no `cx`. The account, the game copies and the launch are read that way in close to two hundred places.
