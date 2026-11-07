---
title: Settings
description: Settings holds the launcher's own options in five categories, shows where Swofty keeps its files and how much room they take, and clears old session logs.
---

# Settings

**Settings**, at the bottom of the sidebar, has a category list on the left and the chosen category on the right. Press Tab to move through the list and Enter to open a category.

| Category | What it holds |
|---|---|
| **General** | Language and starting your game's store |
| **Display** | PC or TV mode |
| **Storage** | The data folder, its size, and cleanup |
| **Advanced** | Developer mode and clearing old logs automatically |
| **About** | The launcher's version, update status and links |

## General

**Language** is a drop-down listing each language by its own name: English, Русский or 简体中文. It changes straight away. The sidebar, the **Play** bar, **Profiles**, **Settings** and the crash screen are translated so far, and everything else stays in English, including the causes the crash screen names. Addons and F1 25 keep their own language.

**Start Steam automatically** (or the EA app) starts your game's store when you press **Play**, as the same item in the **Play** menu does.

## Display

**Display mode** switches between **PC mode**, the normal layout, and **TV mode**, which scales the whole launcher up by 30% and widens the scrollbars, for handhelds and couch play. The first time the launcher opens it asks which one you want, and you can change it here at any time.

## Storage

Storage has three sections.

| Section | Row | What it shows |
|---|---|---|
| **Location** | **Data folder** | The [data folder](/guide/installation#where-your-files-are-stored), where your addons, profiles, backups and logs live. **Open** opens it in File Explorer, or your file manager on Linux. **Move…** moves it to another folder or drive. |
| **Usage** | **Size** | The folder's total size and the free space on its drive. When F1 25's own drive is another one, also the free space there: the drive with your Windows profile, or on Linux the drive with the game's Proton prefix (its Steam library). |
| | **Breakdown** | How much each part takes, largest first: addons, the game backup, session logs, the cache, the career vault, save snapshots, addon data and the rest. |
| **Cleanup** | **Session logs** | How many game sessions are kept. **Clear old** removes the old ones (see below). |
| | **Game drive copy** | The size of the copy of your mods that Play keeps on F1 25's drive (see below). **Clear** deletes it. |

When either drive has less than 3 GB free, **Usage** says so: F1 25 won't start with less than 2 GB free on its `C:` drive, which under Proton is the prefix.

A moved data folder is measured where it really is, through the junction or symbolic link the installer leaves.

**Refresh**, in the **Usage** heading, measures the folder again. Measuring walks every file, so a large addon collection takes a few seconds. Storage measures when you open it.

### Clearing old session logs

Every game session leaves its logs in `sessions` in the data folder, and nothing removed them before. **Clear old** keeps the newest 20 sessions and every session from the last 2 days, and removes the rest. It doesn't run while F1 25 is open. The button names how many sessions it would remove and how much room that frees; it reads **Nothing to clear** when every session is new enough to keep.

**Clear old** asks you to confirm before it deletes anything. The launcher keeps every log unless you turn on **Clear old logs** under Advanced: with it on, the launcher clears old logs by the same rule each time it starts, without asking. It is off by default, so updating never deletes logs.

### The game drive copy

A hard link joins two names for one file and only works inside one drive. Play keeps copies in `swofty-cache/overlay` in the game folder and hard-links the game's files to them. Which files it keeps there depends on where the data folder is:

- On another drive than F1 25, every file Play places.
- On F1 25's drive, the loose files that Play would otherwise copy, such as addon files. Built output still links straight from the data folder. A game that writes to a linked file changes the cached copy and never the addon's original.

The first Play copies every file; later Plays copy only files that are new or whose size or modified time changed, and delete files that are no longer placed. A cached copy the game wrote to is copied again.

The copy stays when the game goes back to stock. It needs the size of those files on F1 25's drive plus 2 GB to spare; with less room, Play copies each file straight into the game folder and the Play log says why. **Clear** removes the copy at any time and doesn't run while F1 25 is open; the next Play rebuilds it.

### Low space when you press Play

When you press **Play** with less than 3 GB free on F1 25's own drive (the drive with your Windows profile, or on Linux the drive with the game's Proton prefix) or on the data folder's drive, the launcher shows a warning. The game still starts: under 2 GB, F1 25 refuses on its own with "cannot run with less than 2048 free storage space".

## Advanced

| Switch | What it does |
|---|---|
| **Developer mode** | Shows the [Tools](/using/tools) page with the `swofty dev` actions, as the same item in the **Play** menu does |
| **Clear old logs** | Off by default. On: clears old session logs when the launcher starts, as **Clear old** does, without asking. Off keeps every log |

## About

Shows the launcher's version, whether an update is available, and links to the documentation and the Discord server.
