---
title: Settings
description: The Settings page shows where Swofty keeps its files and how much room they take, clears old session logs, and holds the launcher's own options.
---

# Settings

**Settings**, at the bottom of the sidebar, has two cards.

## Storage

| Row | What it shows |
|---|---|
| **Data folder** | The [data folder](/guide/installation#where-your-files-are-stored), where your addons, profiles, backups and logs live. **Open** opens it in File Explorer, or your file manager on Linux. |
| **Size** | The folder's total size and the free space on its drive. When F1 25's own drive is another one, also the free space there: the drive with your Windows profile, or on Linux the drive with the game's Proton prefix (its Steam library). |
| The parts | How much each part takes, largest first: addons, the game backup, session logs, the cache, the career vault, save snapshots, addon data and the rest. |
| **Session logs** | How many game sessions are kept. **Clear old** removes the old ones (see below). |
| **Game drive copy** | The size of the copy of your mods that Play keeps on F1 25's drive (see below). **Clear** deletes it. |
| **Another drive** | How to move the data folder. **How to move it** opens [Moving the data folder](/guide/installation#moving-the-data-folder). |

When either drive has less than 3 GB free, the card says so: F1 25 won't start with less than 2 GB free on its `C:` drive, which under Proton is the prefix.

A moved data folder is measured where it really is, through the junction or symbolic link the installer leaves.

**Refresh** measures the folder again. Measuring walks every file, so a large addon collection takes a few seconds.

### Clearing old session logs

Every game session leaves its logs in `sessions` in the data folder, and nothing removed them before. **Clear old** keeps the newest 20 sessions and every session from the last 2 days, and removes the rest. It doesn't run while F1 25 is open. The button names how many sessions it would remove and how much room that frees; it reads **Nothing to clear** when every session is new enough to keep.

**Clear old** asks you to confirm before it deletes anything. The launcher keeps every log unless you turn on **Clear old logs** under General: with it on, the launcher clears old logs by the same rule each time it starts, without asking. It is off by default, so updating never deletes logs.

### The game drive copy

A hard link joins two names for one file and only works inside one drive. Play keeps copies in `swofty-cache/overlay` in the game folder and hard-links the game's files to them. Which files it keeps there depends on where the data folder is:

- On another drive than F1 25, every file Play places.
- On F1 25's drive, the loose files that Play would otherwise copy, such as addon files. Built output still links straight from the data folder. A game that writes to a linked file changes the cached copy and never the addon's original.

The first Play copies every file; later Plays copy only files that are new or whose size or modified time changed, and delete files that are no longer placed. A cached copy the game wrote to is copied again.

The copy stays when the game goes back to stock. It needs the size of those files on F1 25's drive plus 2 GB to spare; with less room, Play copies each file straight into the game folder and the Play log says why. **Clear** removes the copy at any time and doesn't run while F1 25 is open; the next Play rebuilds it.

### Low space when you press Play

When you press **Play** with less than 3 GB free on F1 25's own drive (the drive with your Windows profile, or on Linux the drive with the game's Proton prefix) or on the data folder's drive, the launcher shows a warning. The game still starts: under 2 GB, F1 25 refuses on its own with "cannot run with less than 2048 free storage space".

## General

**Language** sets the language of the launcher's own text: English, Русский or 简体中文. It changes straight away. The sidebar, the **Play** bar, **Profiles**, **Settings** and the crash screen are translated so far, and everything else stays in English, including the causes the crash screen names. Addons and F1 25 keep their own language.

| Switch | What it does |
|---|---|
| **TV mode** | Scales the whole launcher up by 30% and widens the scrollbars, for handhelds and couch play. Off is PC mode, the normal layout. The first time the launcher opens it asks "PC or TV mode?" and saves the answer here. |
| **Start Steam automatically** (or the EA app) | Starts your game's store when you press **Play**, as the same item in the **Play** menu does |
| **Developer mode** | Shows the [Tools](/using/tools) page with the `swofty dev` actions, as the same item in the **Play** menu does |
| **Clear old logs** | Off by default. On: clears old session logs when the launcher starts, as **Clear old** does, without asking. Off keeps every log |
