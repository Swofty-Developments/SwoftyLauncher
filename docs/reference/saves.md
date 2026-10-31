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

When F1 25 is running at that start, the launcher logs that the import waits and tries again at the next start. A profile created later, or one whose store is empty at Play, gets the same copy at its first Play. The Steam save is only read.

### Import and export

```text
$ swofty saves import
imported 2 file(s), 961752 bytes, from /home/you/.local/share/Steam/userdata/843502855/3059520/remote into /home/you/.local/share/swofty/saves/default/remote
the vault holds it as snapshot 20261031-201502-imported

$ swofty saves import --from-profile default --profile progressive
$ swofty saves export --to steam --profile default
the Steam save it replaced is snapshot 20261031-203010-before-restore
exported the launcher saves of profile default into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
start Steam to upload the exported save; if it asks which copy to keep, keep the local files (this computer)
```

The **Career saves** page does the same from its header and details panel: **Import from Steam**, **Export to Steam**, and **Copy careers to…** (another profile's save, or the Steam save from the Vanilla profile). A copy is `launcher_saves::copy_to(data, &from, "progressive", running, now)`; it takes a Steam save or another profile's store as `from`.

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

The session sets `SWOFTY_LAUNCHER_SAVES=off` for the game. To stay on Steam's saves, turn **Use launcher saves** off in the menu beside Play.

### The EA app copy

The core's launcher saves serve Steam's remote storage only. The EA app copy keeps its own saves, and the loader log says so:

```text
[saves] launcher saves are off: launcher saves are not available on the EA copy yet; this session uses the EA copy's own saves
```

The launcher's store keeps a second layout for the EA app copy, `$DATA/saves/<profile>/ea/profile/`, that stays off until the core can serve it.

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

### Steam Cloud status

The status compares each file's SHA-1 with the `sha` Steam recorded in `userdata/<steamid3>/3059520/remotecache.vdf`:

- **synced**: every file matches and Steam marked it synced (`syncstate` 1);
- **waiting to upload**: a file differs from what Steam last synced, so Steam uploads it at its next sync;
- **unknown**: Steam has no record of a file;
- **local only**: the EA app copy and launcher saves, which have no Steam Cloud.

### A save changed outside the launcher

Playing from Steam or the EA app runs F1 25 without the launcher, and a session without an addon's code drops that addon's careers when the game saves. The vault notices the next time it looks at the Steam save: it no longer matches the newest snapshot, and no launcher session explains it. It then takes a `changed-outside` snapshot, pins the snapshot from before the change and logs the event:

```json
{"at":1793448044,"kind":"changed-outside","snapshot":"20261031-120044-changed-outside","from":"20261030-221500-after-session","before":961752,"after":640728,"detail":"found when the launcher started"}
{"at":1793448044,"kind":"shrank","snapshot":"20261031-120044-changed-outside","from":"20261030-221500-after-session","before":961752,"after":640728,"detail":"found when the launcher started"}
```

When the save is also at least 24 KB smaller, the [Career saves](/using/career-saves) page and the Addons page show a notice with **Review the restore** and **Dismiss**, and `swofty run` prints it:

```text
[warning] saves: Your F1 25 save changed outside a launcher session (a Steam or EA app launch) and is 314 KB smaller than before. The career vault kept the version from 2026-10-30 22:15 UTC: 20261030-221500-after-session. If a career is missing, restore it with `swofty saves restore 20261030-221500-after-session` while F1 25 and Steam are closed
```

A save that got 24 KB smaller in a launcher session that left out an addon with `saves.affects` gets the same kind of notice. Size alone doesn't show that a career is gone: ordinary play moves the file by more than that, and the notice only says which version the vault kept.

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
