---
title: Career saves
description: The Career saves page lists the careers in F1 25's save and the versions of the save the launcher keeps, and restores a version with a list of what you get back and lose.
---

# Career saves

The **Career saves** page in the sidebar, next to **Profiles**, shows the careers the launcher has seen in your save and the versions of the profile save it keeps in the [career vault](/reference/saves).

## Careers

Each career is a card with its mode (Driver Career, My Team, Progressive and so on), driver, team, season and round, when you last played it, and the addons it needs. An addon the career needs that is off in the selected profile shows red:

```text
Progressive            In your save
Progressive 2016, Verstappen, Red Bull
season 2016, round 7
Last played 3 hours ago
NEEDS  [Progressive Career is off]
```

The chip on a card is one of:

- **In your save**
- **Missing since <time>**: the career was in the save one launcher session and not in the next
- **Deleted in game**: the game removed it while the launcher watched
- **Archived**

The launcher learns careers from the career index F1 25 writes in a launcher session. Before the first session after the update there are no cards, and the launcher doesn't say any career is missing.

## Versions

Each version shows why it was taken (launcher start, before Play, after Play, F1 25 started outside the launcher, before a restore), the profile, the careers in it, its size and its Steam Cloud status.

**Pin** keeps a version; retention never removes it. **Unpin** lets retention have it again. The launcher also keeps the newest version that still held a career later found missing.

**Restore this save** opens a panel before anything changes:

```text
You get back   Progressive 2016, Verstappen, Red Bull (season 2016, round 7)
You lose       Classic 2020 (not in that version); progress in My Team: season 2, round 9 goes back to season 2, round 4
Also           Settings, setups and everything else in the profile save go back to 2026-10-30 22:15 UTC. The save as it is now is kept as a version first.
```

The panel compares the career index stored with that version with the index of the save as it is. A restore waits until F1 25 and Steam are closed, as in [Restoring a profile save](/using/profiles-and-restoring#restoring-a-profile-save).

## When careers go missing

A banner at the top of the page, and on the **Addons** page, names the missing careers and the time they were last in the save. **Review the restore** opens the panel for that version. **Dismiss** hides the banner until the next career goes missing.

When the launcher has no career index to compare, a save that changed outside a launcher session and got smaller shows the older notice, worded as a change in the save, never as lost careers.

## Storage

The line at the top of the page reads `Vault: 41 versions, 63.0 MB of 500.0 MB.` **Change** sets the size limit; **Open folder** opens the vault. The Steam Cloud row reads `synced`, `waiting to upload` or `unknown`, and says when Steam is running and has to be closed to restore.

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
