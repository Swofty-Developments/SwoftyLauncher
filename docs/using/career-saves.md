---
title: Career saves
description: The Career saves page shows each profile's save, the careers in it and its versions, copies saves between profiles and to and from Steam, and restores a version with a list of what you get back and lose.
---

# Career saves

The **Career saves** page in the sidebar, next to **Profiles**, shows each profile's save, the careers in it and the versions of it the launcher keeps in the [career vault](/reference/saves). Each profile has its own save. Vanilla plays on your Steam save.

## The page

It is laid out like the **Addons** page. The header has **Import from Steam** and **Export to Steam** for the profile you are looking at (the active profile when none is picked). Import and export are disabled, with the reason in the tooltip, for Vanilla, while F1 25 runs, and for export while Steam runs.

```text
Filters                 CAREERS IN PROGRESSIVE                                  Progressive
PROFILE                 Progressive   Verstappen / Red Bull   season 2016       Last played 3 hours ago
 All profiles      4    Driver Career  Hamilton / Mercedes     season 1  Missing Save size   0.9 MB
 Vanilla · Steam   1
 progressive       2    ...                                                     [Copy careers to…]
STATUS                                                                          VERSIONS
 In your save      3                                                            Versions use 2.7 MB of 500 MB
 Missing           1
 Deleted in game   0
MODE
 Progressive       2
```

- **Profile**, **Status** and **Mode** filter the list, and the search box matches a career's name, driver, team, mode and profile.
- Each row is one career: mode, driver and team, season, the profile it is in, when it was last played, the addons it needs (red when off in that profile) and a status chip when it isn't in your save.
- Picking a career opens its details on the right, below the row on a narrow window. **Copy to profile…** copies the whole save of that profile over another profile's save, after a confirm that names both and keeps the target's current save as a version first.
- Picking a profile in the filter with no career selected shows that profile: when it was last played (from its sessions), the size of its save, **Copy careers to…** and its versions. A profile whose save was imported or copied in, with no careers listed yet, shows one entry (`Imported Steam save` or `Copied from <profile>`) that opens the profile; the Profile filter counts it as `Imported`. A profile with no save at all reads `Your careers are in this save. They're listed after you play this profile once.`
- The Vanilla profile plays on your Steam save. Its details show the Steam Cloud status when it is known.

## Versions

The details panel lists the versions of the profile it shows as a timeline, newest first, each with the reason it was kept, its date and time, the careers in it when known, and its size:

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

The panel compares the career index stored with that version with the index of the save as it is. A restore waits until F1 25 is closed, and for the Vanilla save until Steam is closed too, as in [Restoring a profile save](/using/profiles-and-restoring#restoring-a-profile-save). While F1 25 runs the button is disabled and the panel says what to close.

## Storage

Above the versions: `Versions use 2.7 MB of 500 MB`. **Change** sets the size limit; **Open folder** opens that profile's vault.

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
