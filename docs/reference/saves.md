---
title: Saves and the career vault
description: The two places F1 25's profile save can live (Steam's saves and launcher saves), importing and exporting between them, what Steam Cloud carries, and the career vault's snapshots and restores.
---

# Saves and the career vault

F1 25 keeps every career, Grand Prix, Time Trial state, setting and control preset in one **profile save**: two files, the data and a 416-byte index. The launcher can have the game read and write that save in two places.

```text
Steam saves      <Steam>/userdata/<steamid3>/3059520/remote/savegame@profile#*     Steam Cloud syncs it
Launcher saves   $DATA/saves/<profile>/remote/savegame@profile#*                   one per profile, this PC only
```

A profile with **Use launcher saves** on (the default for new and existing profiles) plays on its own launcher saves. Starting F1 25 from Steam, or playing the Vanilla profile, uses the Steam saves.

## Launcher saves

```text
$DATA/saves/
  imported.json                           written once the first-start import has run
  default/remote/savegame@profile#NXDSMWW.BWW
  default/remote/savegame@profile#QKRHMYXE
  progressive/remote/savegame@profile#NXDSMWW.BWW
  progressive/remote/savegame@profile#QKRHMYXE
```

The profile's `swofty/settings.json` names the store, and the core serves the game's profile save from it:

```json
{
  "schema": 1,
  "profile": "default",
  "launcher_saves": {
    "profile": "default",
    "remote": "Z:\\home\\you\\.local\\share\\swofty\\saves\\default\\remote"
  }
}
```

The core wraps the game's save device at its start and replaces the Steam remote storage interface the device holds with a proxy that reads and writes the `remote` folder. Steam and Steam Cloud see none of it. Loader log:

```text
[saves] launcher saves are on for profile default: the game's profile save is served from Z:\home\you\.local\share\swofty\saves\default\remote
[saves] launcher saves armed (profile default): the save device's Steam remote storage (vtable in steam_api64.dll) is replaced by the sandbox; saves are read and written in Z:\...\saves\default\remote; nothing reaches Steam or Steam Cloud
[saves] sandbox: FileWrite savegame@profile#QKRHMYXE (416 bytes) -> written
[saves] sandbox: the game's corrupt-save upload (#1) is off in a launcher saves session; nothing is zipped or uploaded
```

The game's corrupt-save upload to Steam is off in these sessions.

### The first start

The first launcher start after updating to 1.7.0, while F1 25 isn't running, copies the Steam save of the copy Play uses into every profile's store that is empty. A store that already holds files is left alone. Each import is snapshotted in the vault as `Imported from Steam saves`, and `$DATA/saves/imported.json` records that the import ran, so it never repeats on its own:

```text
swofty-launcher: saves: copied the Steam save (2 file(s), 961752 bytes, /home/you/.local/share/Steam/userdata/843502855/3059520/remote) into the launcher saves of profile default (/home/you/.local/share/swofty/saves/default/remote); the vault holds it as snapshot 20261031-201502-imported
swofty-launcher: saves: copied the Steam save (2 file(s), 961752 bytes, ...) into the launcher saves of profile progressive (...)
```

```json
{ "at": 1793477702, "launcher": "1.7.0", "profiles": ["default", "progressive"], "note": "imported from /home/you/.local/share/Steam/userdata/843502855/3059520/remote", "seen": false }
```

The launcher shows one dialog about the import when `profiles` isn't empty and `seen` is `false`, then writes `"seen": true`. `launcher_saves::notice_due` and `launcher_saves::mark_seen` are the two calls.

The Steam save is found under the Steam install's `userdata`, not the game's library: a library on another drive holds `steamapps/common` and no `userdata`. The Steam folder comes from the library's owner in `libraryfolders.vdf`, the default folders, the registry (`SteamPath`, `SteamExe`, `InstallPath`), a running steam.exe, and last the usual Steam folders on every drive. The account is `ActiveUser`, then the `MostRecent` account in `loginusers.vdf`, then the others, and a folder with save files wins over an empty one. When no Steam save is found, the launcher logs every folder it tried and why it was rejected once per start, retries quietly, and profiles play on Steam's saves until an import finds one. The Saves page says so and offers `Play with your Steam saves`, `Import from Steam` and a folder picker.

When F1 25 is running at that start, the launcher logs that the import waits and tries again at the next start. A profile created later, or one whose store is empty at Play, gets the same copy at its first Play. The Steam save is only read.

### Import and export

```text
$ swofty saves import
[info] saves: imported 2 file(s) from /home/you/.local/share/Steam/userdata/843502855/3059520/remote into the launcher saves of profile default
imported 2 file(s), 400416 bytes, from /home/you/.local/share/Steam/userdata/843502855/3059520/remote into /home/you/.local/share/swofty/saves/default/remote
the vault holds it as snapshot 20261031-153037-imported

$ swofty saves import --from-profile default --profile progressive
[info] saves: imported 2 file(s) from /home/you/.local/share/swofty/saves/default/remote into the launcher saves of profile progressive
imported 2 file(s), 400416 bytes, from /home/you/.local/share/swofty/saves/default/remote into /home/you/.local/share/swofty/saves/progressive/remote
the vault holds it as snapshot 20261031-153038-imported-profile

$ swofty saves export --to steam --profile default
the Steam save it replaced is snapshot 20261031-203010-before-restore
exported the launcher saves of profile default into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
start Steam to upload the exported save; if it asks which copy to keep, keep the local files (this computer)
```

Refusals name the reason and change nothing:

```text
$ swofty saves export --to steam
swofty: Steam is running and would call the restored save corrupted. Exit Steam completely (Steam menu > Exit), then restore again. Starting Steam afterwards uploads the restored save.

$ swofty saves import --from foo
swofty: --from takes steam; use --from-profile for another profile

$ swofty saves import --profile vanilla
swofty: vanilla plays without the Swofty core and always uses Steam's saves
```

The **Saves** page does the same from its header: **Import from Steam**, **Export to Steam**, and **Copy careers to…** (another profile's save, or the Steam save from the Vanilla profile). A copy is `launcher_saves::copy_to(data, &from, "progressive", running, now)`; it takes a Steam save or another profile's store as `from`.

Both need F1 25 closed. Export also needs Steam closed. An import keeps what the store held in the vault first, as a `before a restore` snapshot pinned `before-import`. Export goes through the vault's restore, so it snapshots the launcher store, snapshots the Steam save as `before-restore`, checks every file's SHA-256, and never touches `remotecache.vdf`. Only the profile save's files are copied; highlights and ghosts stay where they are. The profile argument defaults to the active profile.

### What Steam Cloud carries

Steam Cloud syncs the Steam saves. Launcher saves stay on the PC they are on: they don't sync between PCs, and Steam never uploads them. After `swofty saves export --to steam`, Steam uploads the exported files at its next start. Import from the Steam save on another PC with `swofty saves import` once Steam has synced it down.

### When launcher saves can't be armed

The core ends F1 25 before the title screen, with exit code 93, when it can't arm launcher saves: a binding that doesn't resolve, a hook that doesn't install, a save device that isn't the expected one, or a store folder that can't be used. No save is read or written.

```text
[saves] LAUNCHER SAVES FAILED: the save device's m_storeInCloud (+0x3e8) is off: it would read and write the player's local save folder (Steam.cloudEnabled is false), which the sandbox does not cover. The game quits now (exit code 93), before the title screen can load a save (profile default); the launcher offers to play once with your Steam saves
```

The core writes the reason to `swofty/launcher-saves.json` in the game folder, and the session ends as stopped. The play bar says `Launcher saves could not be used, so F1 25 closed before loading a save` with **Play once with your Steam saves**, which runs one session with launcher saves off:

```text
$ swofty run --steam-saves-once
```

The session sets `SWOFTY_LAUNCHER_SAVES=off` for the game, through `swofty/session_env.json` on an EA app copy ([EA app](/reference/launch#ea-app)). The core then leaves the profile's launcher saves and their plain store alone. To stay on Steam's saves, turn **Use launcher saves** off in the menu beside Play.

### The EA app copy

The EA app copy's save device reads and writes plain files under `<title folder>\<save>\<file>`. With **Use launcher saves** on, the core points that device at `$DATA/saves/<profile>/ea`, so the profile save is `$DATA/saves/<profile>/ea/profile/`, and the copy's own `%LOCALAPPDATA%\F1 25\savegame\ea` folder is not opened. The first session copies the copy's profile save into an empty store.

The game keeps the title folder twice: in the save system, which opens the files, and in the save system's user interface, which compares the two before every load. The core replaces it in both. With the two different, the game asks `import profile data?` before it loads the profile save.

```text
[saves] launcher saves are on for profile main: the game's profile save is served from Z:\...\saves\main\ea\profile
[saves] launcher saves armed (profile main): the EA save device's title folder C:\users\steamuser\AppData\Local\F1 25\savegame\ea is replaced by Z:\...\saves\main\ea in the save system and in its user interface's copy of the title config (F1 25); saves are read and written there; the EA copy's own savegame folder is not opened
[saves] profile load 1: read from launcher saves (profile main, Z:\...\saves\main\ea\profile); the EA save device reads the profile folder in place
```

If the save device or its user interface is not the expected one, the session ends with exit code 93 before the game loads a save, as on Steam.

### The save the career list reads

The in-game career list (**Career** on the main menu) is built at **Play** from the profile save file the session loads:

| Session | Save file the list reads |
|---|---|
| **Use launcher saves** on, Steam copy | `$DATA/saves/<profile>/remote/savegame@profile#NXDSMWW.BWW` |
| **Use launcher saves** on, EA app copy | `$DATA/saves/<profile>/ea/profile/NXDSMWW.BWW` |
| **Use launcher saves** on, store empty | the copy's own save, as with **Use launcher saves** off |
| **Use launcher saves** off, Steam copy | `userdata/<id3>/3059520/remote/savegame@profile#NXDSMWW.BWW` |
| **Use launcher saves** off, EA app copy | `%LOCALAPPDATA%\F1 25\savegame\ea\profile\NXDSMWW.BWW` |

The session's plain store (the `plain_saves` root in `settings.json`) adds what the core changes at load:

- A career mode the save file has no slot container of is listed from the newest generation a session wrote at a profile save, the one the core puts containers back from.
- A mode a pending restore request names is listed from the request's generation when the request replaces the mode or the save file lacks it.
- With no readable save file, the list shows the careers of the plain store's newest generation.

The build hash takes in the careers the list shows and the names of the drivers and teams the player made. A vault restore, a session, or a save written outside a launcher session changes the list at the next **Play**. The build log names the source:

```text
ui: the Resume Career list screen has 4 career(s); 0 need something this build lacks
ui: the Resume Career list reads C:\Users\you\AppData\Local\F1 25\savegame\ea\profile\NXDSMWW.BWW
ui: the Resume Career list reads /home/you/.local/share/swofty/saves/default/remote/savegame@profile#NXDSMWW.BWW; driver_career_progressive from generation 41 of /home/you/.local/share/swofty/saves/default/plain
```

## Native plain store

With `plain_saves` in `settings.json`, the core writes the plain store of the profile save and puts missing containers back at load.

```json
{"plain_saves": {"root": "Z:\\data\\saves\\default\\plain", "schema": 1}}
```

Without the key the core writes and injects nothing. A key with another `schema`, no `root` or a `..` part in the root logs `[saves] plain store off: <why>` and leaves the store alone. A session with `SWOFTY_LAUNCHER_SAVES=off` and a `launcher_saves` block in `settings.json` runs on the copy's own save while the root is the launcher saves' plain store. The core leaves that store alone and logs `[saves] plain store off: this session runs on the copy's own save (SWOFTY_LAUNCHER_SAVES=off), and the root is the plain store of the profile's launcher saves (<root>); nothing is written or put back`.

The core writes a generation after each finished profile load (`event: "load"`, the tree after any injection) and after each finished profile save (`event: "save"`) on a worker thread, so a save never waits for it. A generation is `generations/<12 digits>/manifest.json` and `containers/<fnv64>.bag`, then `CURRENT`. Every file is written to a temporary name, flushed to disk, renamed, and its folder is flushed. `CURRENT` moves last. A generation number is above `CURRENT` and above every folder on disk, so a number is never reused. The core never deletes a generation.

A `.bag` is one root container as `Game_SaveData_WriteBag` emits it. The load generation holds every root child the game matched to a mode (`source: "game"`, `registered: true`) and every container the splice kept (`source: "kept"`). The save generation holds the kept containers as written and the registered containers as the game wrote them: from the end of the kept entries to the end of the containers block the core copies the bytes the save writes into the stream, splits them by the bag format and keeps them only when exactly the registered children come out whole. When that fails the generation holds the registered containers' load-time bytes marked `"as_of": "load"` and the core logs why. Every manifest container has `pair`: the mode name for `gm_<mode>` and `<mode>_SaveSlotGameModeComponent`, null for any other. A container put back this session has `source: "injected"` in the load generation. `slots` come from the game's slot managers at load; the guid is `0x` and 16 hex digits. `loaded_addons` are the enabled ids in the settings.json `addons` list.

At load the core takes the newest `event: "save"` generation written by a session that had the core and brings back each container the loaded tree lacks. A container whose mode is not registered joins the splice's kept set, so the next profile save writes it back. A container whose mode is registered and that the file lacks is read by the root's registered child from a bag made from our bytes after the root has loaded. A mode reads its container once: its tables keep what a read put in them, so a second read appends to them and runs past their end. The core never reads a container the game has read again, so a container the file holds with no careers stays as the game read it and the core logs `did not put back <mode>: the loaded save holds it without careers ...`; a **Restore** of that mode puts it back at the next Play. The two halves of a mode's pair (`gm_<mode>` and `<mode>_SaveSlotGameModeComponent`) come back or are replaced together from one generation, never one half from another generation; an absent half joins a present one only when the present half has the same SHA-256 as in the generation, otherwise the core logs `did not put back <name>: <why>`. The game's own container with careers is never replaced, and a container the newest save generation lacks is not restored from an older one, so a deletion stays. Only the halves of a mode's pair and `swofty_registry` come back into the kept set; any other container the loaded tree lacks logs `did not put back <name>: it is not a game mode's container`.

Only the profile save has a plain store generation. The game's other saves, such as the race highlights (`savegame@highlights000`), have a root container of the same class with their own registered children; the core tells the profile's root by its `options_options` child (or, where the root's children don't read, by the address of the first root the game loads at boot) and leaves the others alone: no kept containers, no `career.saving` or `career.saved`, no generation, and the kept set stays as the profile load left it. A save generation an older core wrote at a highlights save lists `ReplayHighlights_ReplayHighlights` as its only registered container. The core passes over a save generation whose registered containers include neither `options_options` nor a mode's pair, and logs `plain store: generation <n> was written at another save than the profile's (such as the race highlights); nothing is put back from it`. A profile load that finds `ReplayHighlights_ReplayHighlights` among the profile's own children leaves it out of the kept set and logs `left out ReplayHighlights_ReplayHighlights, the race highlights save's own container, ...`.

```text
[saves] put back driver_career_progressive: 2 career(s) from generation 41
[saves] could not put back driver_career_progressive: generation 41 has no intact copy of it
[saves] plain store: could not capture the registered containers at save: <why>; this generation holds their load-time bytes (as_of load)
[saves] plain store: wrote generation 42 (save, 9 container(s))
[saves] plain store: the save generation was not written (<why>); the game's save is untouched
[saves] plain store: could not capture <name>: <why>
[saves] did not put back ReplayHighlights_ReplayHighlights: it is not a game mode's container
[saves] a save other than the profile (its root registers 1 container(s): ReplayHighlights_ReplayHighlights): no kept save containers, career events or plain store generation for it
```

A container is checked against the size and SHA-256 in its manifest before it is injected.

`inject.json` asks for a restore at the next load: `{"schema":1,"requests":[{"id":"r1","generation":37,"container":"<name>","replace":true}]}`. The core adds an absent container from that generation and, when `replace` is true, replaces a kept or registered container. A registered container is replaced as the root's loop reaches it: the mode reads a bag made from our bytes in place of the one from the file, so it reads its container once. The first half of a mode the loop reaches has every half of the request's generation read into a bag first, so both halves are replaced or neither; a half the file lacks is read by its registered child after the root load. Each replaced half logs `[saves] <name>: the game reads generation <n>'s copy in place of the loaded one`.

The load takes the requests by renaming `inject.json` to `inject.applying.json` before it applies any of them. It writes `injected.json` (`{"schema":1,"results":[{"id":"r1","ok":true,"detail":"..."}]}`) and then deletes `inject.applying.json`; an `inject.json` the launcher wrote while the game ran stays for the next load. Each request logs `[saves] inject request <id>: <detail>` or `[saves] inject request <id> refused: <why>`. A load that stops before it answers (the game closes or crashes while it applies them) leaves `inject.applying.json`, which no later load applies. When the session ends, before the next Play and at launcher start, while F1 25 is closed, the launcher answers each of its requests with `ok: false` and the detail `F1 25 stopped while this restore was applied, so the launcher withdrew it; the next Play loads the save without it`, deletes the file and logs `saves: withdrew restore request <id> (<container> from generation <n>): ...`.

The registered capture, the save capture and the registered put-back run only on the Steam KB build and Steam 1.0.145, where their layouts were read; on any other build, the EA copy included, the core logs one `[build]` line (`the plain store's registered containers off on <exe>: ... is proven on ... only`) and leaves them off. Each step has a kill switch that leaves the game's behaviour as it was: `SWOFTY_PLAIN_REGISTERED=0` leaves the registered containers' load capture out, `SWOFTY_PLAIN_SAVE_CAPTURE=0` leaves the save capture out (the generation then holds the load-time bytes), and `SWOFTY_PLAIN_PUTBACK=0` stops the registered put-back and registered replace requests. A bag or object the core cannot validate is not touched; the core logs `could not put back <name>: <why>` and the game's own container stays.

### Save sandbox writes

A write through the launcher saves proxy is flushed to disk with its folder. The data file and the index are one pair: before a batch replaces a file, the old file is kept as `<name>.swofty-prev` and `batch.swofty-pending` names it. The pending record holds whether the data file and the index were both written. At the next mount or write batch a pair with both written is kept and the old copies go. A pair with only one of them goes back to the old files, and a file the batch created is removed.

```text
[saves] sandbox: a save write was cut off between its data file and its index; the previous pair is back (<files>)
[saves] sandbox: a finished save pair was still marked pending; it is kept (<files>)
```

## Save diagnostics

Every session logs where the game's saves come from, also with launcher saves off. Every profile load logs the source, the save file and each career slot the game read:

```text
[saves] launcher saves are off for profile main: the game reads Steam's saves natively
[saves] profile load 1: read from Steam native; save file not observed (Steam reads it natively)
[saves] profile load 1: manager 0 (vtable +0x5a1c3e0): 2 slot(s)
[saves] profile load 1: manager 0 slot 0: gm_driver_career id=3 key=0000000000abcdef career=dc-000b0000 flags=0x00 setup=2 season=3 weekend=5 date=1761912000 name="Run"
[saves] profile load 1: container swofty_registry: 912 bytes (no loaded game mode claims it)
```

Send Logs lists these lines in the `Saves` section of summary.txt.

## Career vault

The **career vault** keeps copies of the profile save, called **snapshots**, so a career a session dropped can come back. It snapshots the launcher saves and the Steam saves.

A snapshot is taken at launcher start, before every Play, after every Play once the game has closed and its game files are rolled back, and when the launcher sees F1 25 started from Steam or the EA app. A save that hasn't changed since the newest snapshot costs nothing: the vault only logs that it looked.

```text
$ swofty saves list
vault /home/you/.local/share/swofty/career-vault/steam-843502855: 6 snapshot(s), 4.7 MB of 500.0 MB
save /home/you/.local/share/Steam/userdata/843502855/3059520/remote: 2 file(s), 961752 bytes, Steam Cloud: synced
20261031-201502-after-session  after Play, profile default, 2 file(s), 961752 bytes
20261031-183010-before-session  before Play, profile default, 2 file(s), 958120 bytes
20261031-120044-changed-outside  changed outside the launcher, 2 file(s), 640312 bytes
20261030-221500-after-session  after Play, profile progressive, 2 file(s), 961336 bytes, pinned (before-outside-change)
20261029-090000-manual  backup, profile progressive, 2 file(s), 958004 bytes, pinned (backup)
20261027-010203-migrated  from save-backups, profile default, without swofty.progressive, 2 file(s), 929672 bytes

$ swofty saves list --source launcher-default
vault /home/you/.local/share/swofty/career-vault/launcher-default: 3 snapshot(s), 1.8 MB of 500.0 MB
save /home/you/.local/share/swofty/saves/default/remote: 2 file(s), 961752 bytes, Steam Cloud: local only (no Steam Cloud)
20261031-201502-imported  Imported from Steam saves, profile default, 2 file(s), 961752 bytes
```

### Where it lives

The vault is in your [data folder](/guide/installation#where-your-files-are-stored), one folder per Steam account, EA app copy or launcher profile, so saves of different accounts and profiles never mix:

```text
career-vault/
  steam-843502855/                     # userdata/<steamid3> of the account
    blobs/<sha256>                     # one file per distinct save file content, stored as-is
    snapshots/20261031-201502-after-session.json
    events.jsonl                       # what the vault did, one JSON object per line
  ea-steamuser/                        # the EA app copy: ea-<Windows user, or the Wine prefix's user>
  launcher-default/                    # the launcher saves of profile default
  launcher-ea.default/                 # the EA layout of the same store
  migrated.json                        # which save-backups/ folders were moved in
```

The vault reads the save from:

```text
Steam (Windows)    <Steam>\userdata\<steamid3>\3059520\remote\savegame@profile#*
Steam (Linux)      ~/.local/share/Steam/userdata/<steamid3>/3059520/remote/savegame@profile#*
EA app (Windows)   %LOCALAPPDATA%\F1 25\savegame\ea\profile\*
EA app (Linux)     <EA prefix>/drive_c/users/<user>/AppData/Local/F1 25/savegame/ea/profile/*
Launcher saves     $DATA/saves/<profile>/remote/savegame@profile#*
```

Highlights, ghosts and mid-session saves stay out. Steam and the launcher saves store their files under the same names, `profile/NXDSMWW.BWW` and `profile/QKRHMYXE`, so a snapshot of either restores into the other.

### A snapshot

```json
{
  "created": "20261031-201502",
  "at": 1793477702,
  "reason": "after-session",
  "session_id": "4f0c2b9e",
  "profile": "default",
  "launcher": "1.7.0",
  "exe_sha256": "9c1e…",
  "source": "steam-843502855",
  "layout": "steam",
  "save_dir": "/home/you/.local/share/Steam/userdata/843502855/3059520/remote",
  "files": [
    {
      "name": "profile/NXDSMWW.BWW",
      "file": "savegame@profile#NXDSMWW.BWW",
      "size": 961336,
      "sha256": "3b7d…",
      "sha1": "695e7956e05bb22f5b5418b9e86148aa4fa90ced",
      "remotecache": { "size": 961336, "sha": "695e7956e05bb22f5b5418b9e86148aa4fa90ced", "syncstate": "1" }
    },
    {
      "name": "profile/QKRHMYXE",
      "file": "savegame@profile#QKRHMYXE",
      "size": 416,
      "sha256": "a01f…",
      "sha1": "c2d4…",
      "remotecache": { "size": 416, "sha": "c2d4…", "syncstate": "1" }
    }
  ],
  "absent_addons": [],
  "careers_ref": null,
  "pinned": []
}
```

`layout` is `steam`, `ea`, `launcher` or `launcher-ea`. `reason` is one of `launcher-start`, `before-session`, `after-session`, `changed-outside`, `foreign-launch`, `foreign-exit`, `manual`, `before-restore`, `restored`, `migrated`, `imported` and `imported-profile`. `absent_addons` lists the installed addons with [`saves.affects`](/reference/addons/manifest#saves) that the session's profile left out. `remotecache` is Steam's record of the file from `remotecache.vdf` when the snapshot was taken; the launcher only reads that file. A launcher saves snapshot has no `remotecache`.

`swofty saves show <snapshot>` prints the same, with the snapshot's events:

```text
$ swofty saves show 20261030-221500-after-session
20261030-221500-after-session  after Play, profile progressive, 2 file(s), 961336 bytes, pinned (before-outside-change)
taken 2026-10-30 22:15 UTC by launcher 1.7.0
save folder /home/you/.local/share/Steam/userdata/843502855/3059520/remote
session 91aa03d1
F1_25.exe sha256 9c1e…
Steam Cloud when taken: synced
  profile/NXDSMWW.BWW (savegame@profile#NXDSMWW.BWW): 961336 bytes, sha256 3b7d…, sha1 695e…
    remotecache: size 961336, sha 695e…, syncstate 1
  profile/QKRHMYXE (savegame@profile#QKRHMYXE): 416 bytes, sha256 a01f…, sha1 c2d4…
    remotecache: size 416, sha c2d4…, syncstate 1
  event 2026-10-30 22:15 UTC snapshot
  event 2026-10-31 12:00 UTC pinned: before-outside-change
  event 2026-10-31 12:00 UTC changed-outside: found when the launcher started
```

### Careers

Each snapshot stores the core's career index (`$DATA/careers/index.json`, see [career saves](/std/systems/career)) as `index/<sha256>.json` and names it in `careers_ref`. `careers.json` in the vault folder is one row per career id, updated from each snapshot's index:

```json
{
  "version": 1,
  "careers": [
    {
      "career": "progressive:7f3a",
      "mode": "progressive",
      "label": "Progressive 2016, Verstappen, Red Bull",
      "driver": "Verstappen",
      "team": "Red Bull",
      "season": 2016,
      "round": 7,
      "addons": [{ "id": "swofty.progressive" }],
      "first_seen": "20261029-033200-after-session",
      "first_seen_at": 1793244720,
      "last_seen": "20261030-221500-after-session",
      "last_seen_at": 1793398500,
      "last_played": 1793398400,
      "status": "missing-since:20261031-201502-after-session",
      "found_at": 1793477702
    }
  ]
}
```

`status` is one of:

- `present`: in the newest index;
- `missing-since:<snapshot>`: in the index of the snapshot before and not in the index of `<snapshot>`; `last_seen` is the snapshot to restore it from, and retention keeps that snapshot;
- `deleted-in-game`: the index marks the career `deleted`;
- `archived`.

A career is only called missing by comparing two indexes. With no index yet (the first session after the update, the EA app copy before the core ran, the core off) the vault makes no rows and says nothing about careers. The size hint below stays as it is.

`swofty saves versions` prints what restoring each version gets back and loses, from the stored index and the one for the save now:

```text
$ swofty saves versions
20261030-221500-after-session  after Play, profile progressive, 0.9 MB, Steam Cloud: synced, 2 career(s): Progressive 2016, Classic 2020
    restoring it gets back: Progressive 2016 (season 2016, round 7)
    restoring it loses progress in Classic 2020: season 2020, round 9 back to season 2020, round 4
```

`swofty saves pin <snapshot>` adds the pin `pinned`; `unpin` removes only that pin. Pins from a backup, a restore or a missing career stay.

### The plain store

The core writes the launcher's own copy of every save container from inside the game, as plain data: not encrypted, not compressed. The launcher reads it and never writes a generation. `settings.json` hands the core the root:

```json
"plain_saves": { "root": "C:\\Users\\you\\AppData\\Local\\Swofty\\saves\\default\\plain", "schema": 1 }
```

```text
<plain root>/
  CURRENT                         the newest complete generation, decimal text
  generations/000000000041/
    manifest.json                 event, session, containers with their slots
    containers/<fnv64>.bag        one root container's bytes as the game's bag writer emits them
  inject.json                     launcher requests for the core
  inject.applying.json            the requests a profile load took and has not answered yet
  injected.json                   the core's answers
  pins.json                       generations the launcher keeps
```

The root follows the save the session plays:

| Session | Plain root | Vault source |
|---|---|---|
| **Use launcher saves** on, Steam copy | `$DATA/saves/<profile>/plain/` | `launcher-<profile>` |
| **Use launcher saves** on, EA app copy | `$DATA/saves/<profile>/ea-plain/` | `launcher-ea.<profile>` |
| **Use launcher saves** off, Steam copy | `$DATA/plain/steam-<id3>/` | `steam-<id3>` |
| **Use launcher saves** off, EA app copy | `$DATA/plain/ea-<user>/` | `ea-<user>` |

The EA app copy's root sits beside `$DATA/saves/<profile>/ea`, the title folder its save device reads, not inside it. The Steam and EA app copies of one profile never share a root: what one copy wrote is never put back into the other's save and never counts as a career the other lost. A session on the copy's own save names the root of the copy save the launcher found last, and no root while that save is the other copy's. A session run once on the copy's own save with launcher saves on writes none. A profile without `plain_saves` in its settings writes and injects nothing.

#### Splitting a shared store

Before launcher 1.10.15, a session on either copy wrote `$DATA/saves/<profile>/plain/`. Each manifest names the exe that wrote it in `exe_sha256`, and every build a core runs on is in `re/db/builds/` (`ea.json` and `ea-1-0-145.json` for the EA app copy). Once per profile, at launcher start and before a Play, with F1 25 closed and the session lock held, the launcher splits that store:

1. Each complete generation an EA app build wrote is copied into `$DATA/saves/<profile>/ea-plain/generations/`, under the same number. A copy goes to `ea-plain/split.copying/` first and is renamed into place whole. `ea-plain/CURRENT` then names the newest one.
2. Each restore request in `plain/inject.json` or `plain/inject.applying.json` that names one of them moves to the same file in `ea-plain/`, with the same generation number, and `ea-plain/pins.json` pins that generation. The next EA app session takes it. A request whose generation `ea-plain/` has no copy of (no generation under that number, or one of its own) is answered in `ea-plain/injected.json` as not applied, naming the generation and where it is kept. `ea-plain/` is written first and `plain/` last, each id once. Requests for any other generation stay in `plain/`.
3. Each of them then moves out of `plain/generations/` to `plain/split-ea/generations/`, where no reader looks. The Steam layout's store keeps its `CURRENT`, so the next generation's number is still above every earlier one.
4. `plain/split.json` records the split: the EA app generations, whether the EA layout's store got a copy, and the generations no known build wrote.

```text
saves: split the plain store of profile default: 1 generation(s) the EA app copy wrote (40) are set aside in /home/you/.local/share/swofty/saves/default/plain/split-ea; /home/you/.local/share/swofty/saves/default/ea-plain has a copy of each
saves: moved restore request r1795230000-gm_f1_world (gm_f1_world from generation 40) of profile default to inject.json of the EA layout's store /home/you/.local/share/swofty/saves/default/ea-plain; the next EA app session takes it
```

A store split by launcher 1.10.15 moves its leftover requests for the generations `split.json` names at the next launcher start or Play.

Nothing is deleted. A generation no known build wrote, and a torn one, stays in `plain/`, the Steam layout's store. An `ea-plain/` that a core wrote before the split gets no copy: its own generations are newer than any copy would be, and the set-aside ones stay in `plain/split-ea/`. A split that stops part way runs again from where it stopped at the next launcher start or Play; `split.json` is written last.

The core writes a generation after every profile load the game finishes (`event: "load"`) and after every profile save the game reports done (`event: "save"`). `packages/core/src/games/f1_25/saves/plain/` reads it:

```rust
let plain = Plain::of_source(&data, "launcher-default");
let newest = plain.newest();      // Option<Manifest>, never newer than CURRENT
let careers = plain.careers();    // one row per slot of the newest generation
```

A generation counts when it is not newer than `CURRENT`, its manifest parses, and every container file it lists is there with the length the manifest states. A torn or partial generation is skipped, and the next older complete one stands in.

The launcher prunes generations at launcher start, never while F1 25 runs or a session holds the lock. It keeps the newest 60, `CURRENT`, every generation in `pins.json` (a restore request pins its generation) and the newest `save` generation of each session.

### Careers the newest generation lacks

A profile with **Use launcher saves** off keeps its careers in the Steam save. An F1 25 started from Steam without the launcher runs without the profile's addons and drops the career containers no loaded game mode claims (`gm_driver_career_progressive`) when it saves; Steam Cloud then uploads that save.

Every mode is two root containers, `gm_<mode>` and `<mode>_SaveSlotGameModeComponent`, and the launcher treats them as one unit. A manifest entry names its mode in `pair`; without it the launcher strips a leading `gm_` or a trailing `_SaveSlotGameModeComponent`. The launcher decides what is lost by identity: the mode, then the career's slot guid. A mode or career is lost when the last good save generation has it and the newest generation lacks it; a mode with one half missing is lost. The last good save generation is the newest `save` generation a session with the core wrote. Only a `load` generation newer than it can show a loss, because a career missing from a later `save` was deleted in a session with the core, and deletions stick. A container the core put back at load is in the newest generation with `source: "injected"`, and a container named in a `[saves] put back <name>: <k> career(s) from generation <g>` line of the newest session's loader log is not lost. No byte total takes part.

Only modes with careers count: a mode whose containers list career slots, or one the core keeps unclaimed for an addon (the game reads no slots of it). The game's own records (`options_options`, `trophies_trophies`, journal entries) and modes with no career (`gm_dev`, `gm_time_trial`) never raise the notice. A container the newest load kept unclaimed (its addon off) still holds its careers and is not lost. A load generation that lists no registered container, as on the EA app copy where the core captures only the containers it keeps or puts back, judges only the containers the good generation kept. A career lost from a mode the game reads by slot is named by its slot; a mode whose slots the game does not read is named by its label (`Driver Career Progressive`).

```rust
// packages/core/src/games/f1_25/saves/vault/guard.rs
let loss = vault.probable_loss();   // Option<Loss { snapshot, at, plain, .. }>
```

The launcher checks at its start, before Play, when F1 25 closes outside it and after a session. The Career Saves page and the Addons page show the careers, the date of that generation and **Restore**; the play bar shows the same line with **Restore** and **Open Career saves**. **Restore** asks the core to put the lost containers back (below). **Dismiss** hides the banner for that pair of generations. `swofty run` prints the same notice:

```text
[warning] saves: Progressive 2016, season 2016, round 7 is in the last save a launcher session wrote but not in the save the game read since. The launcher's plain store keeps it in generation 41. Put it back with `swofty saves restore-career driver_career_progressive`; the game applies it at the next Play
```

The vault's encrypted whole-file snapshots stay as the backstop: every launcher session on the Steam save writes `before-session` and `after-session` snapshots to the Steam vault (`steam-<id3>`), whatever the profile's launcher saves setting, and `swofty saves restore` puts a whole save back.

With launcher saves off and an addon career in the index, the Saves page shows a warning, and **Use launcher saves** when the profile's launcher store is empty:

```rust
guard::steam_save_warning(vanilla, profile.launcher_saves, &careers::addon_careers(&index, Some("progressive")))
```

### Restore one career

The Career Saves page lists the careers of the newest generation grouped by mode: name, season and round, last played time, and where each lives (`In the game's save`, `Kept by the launcher` for a container the game's modes don't claim, `Put back by the launcher`). **Restore** on a mode lists up to six earlier versions of it, newest first; choosing one writes `inject.json` with a request for each of the mode's two containers from that generation and pins it:

```json
{"schema":1,"requests":[
  {"id":"r1795000000-gm_driver_career_progressive","generation":37,"container":"gm_driver_career_progressive","replace":true},
  {"id":"r1795000000-driver_career_progressive_SaveSlotGameModeComponent","generation":37,"container":"driver_career_progressive_SaveSlotGameModeComponent","replace":true}
]}
```

At the next profile load the core takes the requests (`inject.applying.json`), applies each one, writes `injected.json` and deletes `inject.applying.json`. The row shows **Restore waits for Play** until then, and the page shows the core's answer afterwards:

```json
{"schema":1,"results":[{"id":"r1795000000-gm_driver_career_progressive","ok":true,"detail":"1 career(s) from generation 37"}]}
```

The command line does the same:

```text
$ swofty saves plain
generation 41 (save at 2026-11-19 10:02 UTC), 2 container(s)
driver_career_progressive	0x0123456789abcdef	Jacob Nardella	2026-11-19 10:02 UTC
$ swofty saves restore-career driver_career_progressive --generation 37
```

The whole-save restore (`swofty saves restore`, and **Restore** on a version in the list) stays as the fallback: it needs F1 25 and Steam (or the EA app) closed, and replaces every container.

### Steam Cloud status

The status compares each file's SHA-1 with the `sha` Steam recorded in `userdata/<steamid3>/3059520/remotecache.vdf`:

- **synced**: every file matches and Steam marked it synced (`syncstate` 1);
- **waiting to upload**: a file differs from what Steam last synced, so Steam uploads it at its next sync;
- **unknown**: Steam has no record of a file;
- **local only**: the EA app copy and launcher saves, which have no Steam Cloud.

### A save changed outside the launcher

Playing from Steam or the EA app runs F1 25 without the launcher. The vault notices the next time it looks at the Steam save: it no longer matches the newest snapshot, and no launcher session explains it. It then takes a `changed-outside` snapshot, pins the snapshot from before the change and logs the event:

```json
{"at":1793448044,"kind":"changed-outside","snapshot":"20261031-120044-changed-outside","from":"20261030-221500-after-session","detail":"found when the launcher started"}
```

The change alone raises no notice. The file's size decides nothing: a notice comes from [the plain store](#careers-the-newest-generation-lacks) when a container or career is missing.

### A save that stalls at load

A profile save last played with addon content that is off now (a career, livery or item from an addon the profile or Vanilla leaves out) can keep F1 25 on Loading... after Press any button: the spinner turns and team radio plays, and the main menu never comes. It does the same from Steam or the EA app without the launcher, through Verify and reinstalls, because the save is the cause.

The save is encrypted, so the launcher learns such a save from its sessions. A session stalled on the save when the core logged a profile load and the session's perf.log (kept from `state/perf-fallback.log` in the data folder when the game can't write in its folder) ends on `screen splash_transition` for 120 s or more (99 in 100 menu loads take under 82 s), with no addon known to stall the boot on and no [withdrawn](/using/addons#withdrawn) addon on. Two such sessions on the same save, or one of 15 minutes, make it a stalling save, keyed by the SHA-256 of `savegame@profile#NXDSMWW.BWW` from the session's `before-session` snapshot. A save that any session got past the boot screens with, or that the game wrote during the session, never counts. `$DATA/state/stalled-saves.json` keeps what was learned and offered:

```json
{
  "saves": [
    { "sha256": "9f1c…", "size": 932920, "sessions": ["20261104-023037", "20261104-023146"], "secs": 162, "profile": "colorgrading-testing", "found_at": 1793770000 }
  ],
  "moved": [
    { "source": "launcher-colorgrading-testing", "sha256": "9f1c…", "kept": "20261104-023003-manual", "offer": "20261101-123709-after-session", "dismissed": false, "at": 1793770000 }
  ]
}
```

At launcher start and before every Play, `stall::sweep` looks at every profile's launcher saves and the Steam or EA app save of the copy Play uses. It never swaps or empties a save. A stalling save gets a snapshot pinned `stalled-at-load`, and the newest snapshot of the same vault that isn't a stalling save becomes the offer. The Career Saves page shows a banner with **Restore** and **Dismiss**; the player decides. **Restore** is the whole-save restore of the offered snapshot, which keeps the save it replaces as `before-restore`. **Dismiss** leaves the save as it is. With no earlier snapshot the banner says so and offers no restore. Each find is one line:

```text
saves: the profile save in C:\Users\you\AppData\Local\Swofty\saves\colorgrading-testing\remote kept F1 25 on Loading... after Press any button; the career vault keeps a pinned snapshot of it (20261104-023003-manual) and the Career Saves page offers the version from 2026-11-01 12:37 UTC (20261101-123709-after-session); the save is left as it is
```

A save the sweep found once is not flagged again. Import, Copy careers to… and Export to Steam refuse a stalling save:

```text
swofty: the profile save in /home/you/.local/share/Steam/userdata/843502855/3059520/remote kept F1 25 on Loading... after Press any button, so the launcher does not copy it; the career vault keeps it to restore once the addons it was saved with are back
```

A session that read the profile save and then sat on the start screen for 120 s or more shows the known problem `stalled_save` (**F1 25 stayed on the loading screen after reading your save**) in the crash window.

### A save stuck in a started race weekend

At every Team HQ entry the core logs the career's current weekend ([Weekend diagnostics](/reference/std/career#weekend-diagnostics)). A weekend past not started with no session complete is logged as `weekend already started, session never ran`. A load of such a save opens Team HQ with no Advance, and the weekend's first session never loads.

The launcher learns such a save from its sessions. A session counts when its last Team HQ weekend line is `weekend already started, session never ran` and no `StateLoadSession` comes after it. The save it learns is the one the session ended on, keyed by the SHA-256 of `savegame@profile#NXDSMWW.BWW` in the session's `after-session` snapshot. Each session folder is read once, after its `after-session` snapshot is in the vault. `$DATA/state/stuck-weekends.json` keeps what was learned:

```json
{
  "saves": [
    { "source": "launcher-kok", "sha256": "4be1…", "size": 932920, "weekend": { "round": 12, "rounds": 22, "session": "practice_1", "track": "game/hungaroring" }, "sessions": ["20261120-104632", "20261120-105635"], "profile": "kok", "found_at": 1795173800 }
  ],
  "read": ["20261120-104632", "20261120-105635", "20261120-111910"]
}
```

`stuck::sweep` runs at launcher start and after every Play. Each find is one line:

```text
saves: session 20261120-104632 ended with its career in the round 12 of 22 (hungaroring) weekend, started with no session run; the profile save 4be1c0d2a9f3 (932920 bytes) of launcher-kok holds it, and the Career Saves page offers an earlier version
```

While a learned save is in its save folder, the Career Saves page and the Addons page show a banner that names the round, with **Restore**, **Review the restore** and **Dismiss**. The offer is the newest snapshot of the same vault that is not a learned save. **Restore** is the whole-save restore of the offered snapshot, which keeps the save it replaces as `before-restore`; every career in the save goes back to that version. **Dismiss** leaves the save as it is. With no other version the banner says so and offers no restore. The launcher never changes the save by itself.

A session that ended this way shows the known problem `stuck_weekend` (**Your career is stuck at the start of a race weekend**) in the crash window, also when F1 25 then hit its error on closing.

### Back up and restore

```text
$ swofty saves backup
saved and pinned snapshot 20261031-170210-manual (961752 bytes)

$ swofty saves restore 20261030-221500-after-session
[info] [vault] restored 20261030-221500-after-session into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
the save it replaced is snapshot 20261031-120044-changed-outside
restored 20261030-221500-after-session into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
start Steam to upload the restored save; if it asks which copy to keep, keep the local files (this computer)

$ swofty saves restore 20261031-201502-imported --source launcher-default
[info] [vault] restored 20261031-201502-imported into /home/you/.local/share/swofty/saves/default/remote
the save it replaced is snapshot 20261031-201502-imported
restored 20261031-201502-imported into /home/you/.local/share/swofty/saves/default/remote
the launcher saves are restored; Steam's saves are not touched
```

`backup` pins its snapshot, so retention never removes it. A restore:

1. refuses while a Play session or another restore holds the session lock, while F1 25 runs, and while Steam runs for a Steam save (`Exit Steam completely (Steam menu > Exit), then restore again`) or the EA app runs for an EA save; a launcher saves restore only needs F1 25 closed;
2. checks every file of the snapshot against its SHA-256;
3. snapshots the current save as `before-restore` and pins it;
4. copies every file to a temporary name beside its target and checks its hash, then renames them all into place; when a copy fails nothing is renamed, the temporary files go and the save stays as it was; when a rename fails the error gives the `swofty saves restore <id>` command for the `before-restore` snapshot;
5. leaves `remotecache.vdf` as it is, and leaves profile files the snapshot doesn't have, such as F1 1990's `savegame@profile#D1990.UEL`;
6. logs `restored` in `events.jsonl` and takes a `restored` snapshot.

The next Steam start uploads the restored files, and `swofty saves list` shows **synced** once it has.

Pass `--source steam-<id>`, `--source ea-<user>` or `--source launcher-<profile>` to `list`, `show` and `restore` to use another vault.

### Retention

After each snapshot the vault keeps:

- every snapshot from the last 48 hours, at most 40;
- the newest of each day until snapshots are two weeks old;
- the newest of each week for eight more weeks;
- every pinned snapshot, and the newest one.

Over the size cap the oldest unpinned snapshots go first; when only pinned snapshots and the newest one are left, the vault stays over the cap and logs it in `events.jsonl`. A prune that cannot read a file in `snapshots/` (for example one written by a newer launcher) removes nothing and logs the file names. A vault lock that cannot be taken stops the snapshot or prune with an error. Pins come from `swofty saves backup` (`backup`), a restore (`before-restore`), an import over a store that held files (`before-import`), an export (`export`), a change outside the launcher (`before-outside-change`), F1 25 started outside the launcher (`foreign-launch`) and a Vanilla-profile Play with addons that affect saves installed (`vanilla-session`).

```text
$ swofty saves cap
500 MB
$ swofty saves cap 800
the career vault keeps at most 800 MB; the next snapshot prunes to it
```

The cap is `vault_cap_mb` in `config.json`.

### The old save-backups folder

Launchers before the vault kept up to 10 copies in `$DATA/save-backups/`. The first vault check moves each one in as a `migrated` snapshot pinned `migrated`, so no prune or cap removes it, and records it in `career-vault/migrated.json`. After a prune first removes a snapshot, each recorded folder is deleted if its snapshot and all its blobs are in the vault with the right hash. Folders not recorded in `migrated.json` stay, and `save-backups/` is removed only when it is empty.
