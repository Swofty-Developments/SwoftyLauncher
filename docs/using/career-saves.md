---
title: Career saves
description: The Career saves page shows each profile's save, the careers in it and its versions, copies saves between profiles and to and from Steam, and restores a version with a list of what you get back and lose.
---

# Career saves

The **Career saves** page in the sidebar, next to **Profiles**, shows each profile's save, the careers in it and the versions of it the launcher keeps in the [career vault](/reference/saves). Each profile has its own save. Vanilla plays on your Steam save.

## Profiles

Each profile is a card, the active one first:

```text
progressive   [Active]                                          Last played 3 hours ago
Progressive   Progressive 2016 · Verstappen · Red Bull · season 2016, round 7 · played 3 hours ago
[Copy careers to…]  [Import from Steam]  [Export to Steam]                      [Versions]

vanilla   [Steam save]                                          Last played 2 days ago
Driver Career   Classic 2020 · Hamilton · Mercedes · season 1, round 4 · played 2 days ago
Steam Cloud: synced
[Copy careers to…]                                                               [Versions]
```

A profile with no careers known yet reads `Careers show here after you play this profile`. The launcher learns careers from the career index F1 25 writes in a launcher session, and never says a career is missing before it has seen one.

- **Copy careers to…** lists the other profiles. Choosing one asks first, naming both profiles, and copies this profile's save over the target's. The target's current save is kept as a version first.
- **Import from Steam** copies your Steam save over this profile's save, with the same confirm and the same kept version.
- **Export to Steam** copies this profile's save over your Steam save, after a confirm, and keeps the Steam save as a version first. It needs Steam and F1 25 closed.
- **Versions** shows that profile's versions below the cards.

Copy and import need F1 25 closed. A button that has to wait is disabled and the card says what to close.

## Versions

The versions of the selected profile are a timeline, newest first, each with the reason it was kept, its date and time, the careers in it when known, and its size:

```text
After Play                     3 hours ago · 2026-10-31 21:40 UTC
1 career: Progressive 2016 · 0.9 MB                                    [Pin] [Restore]
Before Play                    3 hours ago · 2026-10-31 18:55 UTC
Imported from Steam            2 days ago · 2026-10-29 09:12 UTC
Old backup                     3 days ago · 2026-10-28 20:01 UTC
```

The reasons are Before Play, After Play, Imported from Steam, Copied from another profile, Launcher start, Changed outside the launcher, Before restore, Restored, Old backup (a migrated `save-backups` copy) and Backup.

**Pin** keeps a version; retention never removes it. **Unpin** lets retention have it again. The launcher also keeps the newest version that still held a career later found missing.

**Restore** opens a panel before anything changes:

```text
You get back   Progressive 2016, Verstappen, Red Bull (season 2016, round 7)
You lose       Classic 2020 (not in that version); progress in My Team: season 2, round 9 goes back to season 2, round 4
Also           Settings, setups and everything else in the profile save go back to 2026-10-30 22:15 UTC. The save as it is now is kept as a version first.
```

The panel compares the career index stored with that version with the index of the save as it is. A restore waits until F1 25 is closed, and for the Vanilla save until Steam is closed too, as in [Restoring a profile save](/using/profiles-and-restoring#restoring-a-profile-save). While F1 25 runs the button reads `Close F1 25 to restore` and is disabled.

## Storage

Under the versions: `Versions use 2.7 MB of 500 MB`. **Change** sets the size limit; **Open folder** opens that profile's vault. The Vanilla card shows `Steam Cloud: synced`, `waiting to upload` or `local only` when the launcher knows it.

## The first time

The first launcher start of 1.7.0 copies your Steam save into every profile's save and opens one dialog:

```text
Your careers have their own saves now
Each profile now has its own save, starting from a copy of your Steam save. Playing from Steam still uses your Steam save, and Vanilla plays on it too.
[Show me]  [OK]
```

**Show me** opens Career saves. The dialog is recorded as seen in `saves/imported.json` (`"seen": true`) and never shows again.

## When careers go missing

A banner at the top of the page, and on the **Addons** page, names the missing careers, the profile whose save they went missing from, and the time they were last in the save. **Review the restore** opens that profile's versions with the panel for that version. **Dismiss** hides the banner until the next career goes missing.

When the launcher has no career index to compare, a save that changed outside a launcher session and got smaller shows the older notice, worded as a change in the save, never as lost careers.

## From a terminal

```text
$ swofty saves careers
p:1  Progressive 2016, progressive, Verstappen, Red Bull, season 2016, round 7, needs swofty.progressive, missing-since:20261031-201502-after-session
    last in the save at snapshot 20261030-221500-after-session

$ swofty saves versions
2 version(s) in /home/you/.local/share/swofty/career-vault/steam-843502855, 1.9 MB of 500.0 MB
Steam Cloud now: synced
20261031-201502-after-session  after Play, profile default, 0.9 MB, Steam Cloud: synced, 1 career(s): Classic 2020
20261030-221500-after-session  after Play, profile progressive, 0.9 MB, Steam Cloud: synced, 2 career(s): Progressive 2016, Classic 2020, pinned (holds-missing-career)
    restoring it gets back: Progressive 2016 (season 2016, round 7)

$ swofty saves pin 20261030-221500-after-session
pinned snapshot 20261030-221500-after-session
$ swofty saves unpin 20261030-221500-after-session
unpinned snapshot 20261030-221500-after-session; it stays kept for holds-missing-career
```
