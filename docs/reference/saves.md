---
title: Career vault
description: Where the launcher keeps copies of F1 25's profile save, when it takes them, and the swofty saves commands that list, back up and restore them.
---

# Career vault

F1 25 keeps every career, Grand Prix, Time Trial state, setting and control preset in one **profile save**: two files, the data and a 416-byte index. The **career vault** keeps copies of that save, called **snapshots**, so a career a session dropped can come back.

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
```

## Where it lives

The vault is in your [data folder](/guide/installation#where-your-files-are-stored), one folder per Steam account or EA app copy, so saves of different accounts never mix:

```text
career-vault/
  steam-843502855/                     # userdata/<steamid3> of the account
    blobs/<sha256>                     # one file per distinct save file content, stored as-is
    snapshots/20261031-201502-after-session.json
    events.jsonl                       # what the vault did, one JSON object per line
  ea-steamuser/                        # the EA app copy: ea-<Windows user, or the Wine prefix's user>
  migrated.json                        # which save-backups/ folders were moved in
```

The vault reads the save from:

```text
Steam (Windows)   <Steam>\userdata\<steamid3>\3059520\remote\savegame@profile#*
Steam (Linux)     ~/.local/share/Steam/userdata/<steamid3>/3059520/remote/savegame@profile#*
EA app (Windows)  %LOCALAPPDATA%\F1 25\savegame\ea\profile\*
EA app (Linux)    <EA prefix>/drive_c/users/<user>/AppData/Local/F1 25/savegame/ea/profile/*
```

Highlights, ghosts and mid-session saves stay out. Both layouts store their files under the same names, `profile/NXDSMWW.BWW` and `profile/QKRHMYXE`.

## A snapshot

```json
{
  "created": "20261031-201502",
  "at": 1793477702,
  "reason": "after-session",
  "session_id": "4f0c2b9e",
  "profile": "default",
  "launcher": "1.6.12",
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

`reason` is one of `launcher-start`, `before-session`, `after-session`, `changed-outside`, `foreign-launch`, `foreign-exit`, `manual`, `before-restore`, `restored` and `migrated`. `absent_addons` lists the installed addons with [`saves.affects`](/reference/addons/manifest#saves) that the session's profile left out. `remotecache` is Steam's record of the file from `remotecache.vdf` when the snapshot was taken; the launcher only reads that file.

`swofty saves show <snapshot>` prints the same, with the snapshot's events:

```text
$ swofty saves show 20261030-221500-after-session
20261030-221500-after-session  after Play, profile progressive, 2 file(s), 961336 bytes, pinned (before-outside-change)
taken 2026-10-30 22:15 UTC by launcher 1.6.12
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

## Steam Cloud status

The status compares each file's SHA-1 with the `sha` Steam recorded in `userdata/<steamid3>/3059520/remotecache.vdf`:

- **synced**: every file matches and Steam marked it synced (`syncstate` 1);
- **waiting to upload**: a file differs from what Steam last synced, so Steam uploads it at its next sync;
- **unknown**: Steam has no record of a file;
- **local only**: the EA app copy, which has no Steam Cloud.

## A save changed outside the launcher

Playing from Steam or the EA app runs F1 25 without the launcher, and a session without an addon's code drops that addon's careers when the game saves. The vault notices the next time it looks: the save no longer matches the newest snapshot, and no launcher session explains it. It then takes a `changed-outside` snapshot, pins the snapshot from before the change and logs the event:

```json
{"at":1793448044,"kind":"changed-outside","snapshot":"20261031-120044-changed-outside","from":"20261030-221500-after-session","before":961752,"after":640728,"detail":"found when the launcher started"}
{"at":1793448044,"kind":"shrank","snapshot":"20261031-120044-changed-outside","from":"20261030-221500-after-session","before":961752,"after":640728,"detail":"found when the launcher started"}
```

When the save is also at least 24 KB smaller, the Addons page shows a notice with **Restore** and **Dismiss**, and `swofty run` prints it:

```text
[warning] saves: Your F1 25 save changed outside a launcher session (a Steam or EA app launch) and is 314 KB smaller than before. The career vault kept the version from 2026-10-30 22:15 UTC: 20261030-221500-after-session. If a career is missing, restore it with `swofty saves restore 20261030-221500-after-session` while F1 25 and Steam are closed
```

A save that got 24 KB smaller in a launcher session that left out an addon with `saves.affects` gets the same kind of notice. Size alone doesn't show that a career is gone: ordinary play moves the file by more than that, and the notice only says which version the vault kept.

## Back up and restore

```text
$ swofty saves backup
saved and pinned snapshot 20261031-170210-manual (961752 bytes)

$ swofty saves restore 20261030-221500-after-session
[info] [vault] restored 20261030-221500-after-session into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
the save it replaced is snapshot 20261031-120044-changed-outside
restored 20261030-221500-after-session into /home/you/.local/share/Steam/userdata/843502855/3059520/remote
start Steam to upload the restored save; if it asks which copy to keep, keep the local files (this computer)
```

`backup` pins its snapshot, so retention never removes it. A restore:

1. refuses while a Play session or another restore holds the session lock, while F1 25 runs, and while Steam runs for a Steam save (`Exit Steam completely (Steam menu > Exit), then restore again`) or the EA app runs for an EA save;
2. checks every file of the snapshot against its SHA-256;
3. snapshots the current save as `before-restore` and pins it;
4. copies every file to a temporary name beside its target and checks its hash, then renames them all into place; when a copy fails nothing is renamed, the temporary files go and the save stays as it was; when a rename fails the error gives the `swofty saves restore <id>` command for the `before-restore` snapshot;
5. leaves `remotecache.vdf` as it is, and leaves profile files the snapshot doesn't have, such as F1 1990's `savegame@profile#D1990.UEL`;
6. logs `restored` in `events.jsonl` and takes a `restored` snapshot.

The next Steam start uploads the restored files, and `swofty saves list` shows **synced** once it has.

Pass `--source steam-<id>` or `--source ea-<user>` to `list`, `show` and `restore` to use another account's vault.

## Retention

After each snapshot the vault keeps:

- every snapshot from the last 48 hours, at most 40;
- the newest of each day until snapshots are two weeks old;
- the newest of each week for eight more weeks;
- every pinned snapshot, and the newest one.

Over the size cap the oldest unpinned snapshots go first; when only pinned snapshots and the newest one are left, the vault stays over the cap and logs it in `events.jsonl`. A prune that cannot read a file in `snapshots/` (for example one written by a newer launcher) removes nothing and logs the file names. A vault lock that cannot be taken stops the snapshot or prune with an error. Pins come from `swofty saves backup` (`backup`), a restore (`before-restore`), a change outside the launcher (`before-outside-change`), F1 25 started outside the launcher (`foreign-launch`) and a Vanilla-profile Play with addons that affect saves installed (`vanilla-session`).

```text
$ swofty saves cap
500 MB
$ swofty saves cap 800
the career vault keeps at most 800 MB; the next snapshot prunes to it
```

The cap is `vault_cap_mb` in `config.json`.

## The old save-backups folder

Launchers before the vault kept up to 10 copies in `$DATA/save-backups/`. The first vault check moves each one in as a `migrated` snapshot pinned `migrated`, so no prune or cap removes it, and records it in `career-vault/migrated.json`. After a prune first removes a snapshot, each recorded folder is deleted if its snapshot and all its blobs are in the vault with the right hash. Folders not recorded in `migrated.json` stay, and `save-backups/` is removed only when it is empty.
