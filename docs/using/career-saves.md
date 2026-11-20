---
title: Saves
description: The Saves page under Profiles shows the active profile's save, its careers and versions, copies saves between profiles and to and from Steam, and restores a version.
---

# Saves

**Saves** is a page under **Profiles** in the sidebar. It shows the save of the active profile; switch profiles with the picker at the bottom left to see another profile's save. Vanilla plays on your Steam save.

```text
progressive saves                         [Copy careers to…] [Import from Steam] [Export to Steam]
This profile's own save

CAREERS
Driver Career   Oscar PIASTRI, McLaren                                     3 hours ago
                Season 2, round 4
My Team         Racing Bulls                                    Deleted in game

VERSIONS
After Play            3 hours ago      0.9 MB                          [Pin] [Restore]
Before Play           3 hours ago      0.9 MB                          [Pin] [Restore]
Imported from Steam   2 days ago       0.9 MB                          [Pin] [Restore]

Versions use 2.7 MB of 500 MB · Change · Open folder
```

- **Copy careers to…** copies this profile's whole save over another profile's save. A confirm names both and the target's current save is kept as a version.
- **Import from Steam** copies your Steam save over this profile's save, with the same confirm and kept version.
- **Export to Steam** copies this profile's save over your Steam save. It needs Steam and F1 25 closed.
- A disabled action says why on hover.
- A save that was imported or copied in, with no careers listed yet, shows one row: `Imported Steam save · careers listed after you play this profile once`.
- A career has a chip only when it is **Missing** or **Deleted in game**.

## Resume Career

**Career** on the main menu opens a list of your careers, newest first, with **New Career** at the top. The list is on by default.

```text
CAREER                                                                    10 careers

NEW CAREER
Nigel MANSELL      Ferrari                  F1 1990 Driver      Season 1, Race 1      Ready
Lando NORRIS       McLaren Mastercard...    F1® 2026 Driver     Season 1, Race 1      Ready
John BENNETT       wefew                    F1® 2026 My Team    Season 1, Race 1      Missing
```

- A row shows the driver, the team, the content and mode, the season and race, and the status on the right.
- The strip on the left of a row is the team's colour.
- Focus a row to see its team, season and race, when it was last played and what it needs.
- The list shows the saves that have a driver and a team. A save slot the game would not offer to continue is not listed.
- A row's status is one of:
  - `Ready`: the profile has every addon the career needs.
  - `Missing`: an addon is off or not installed (`Missing: <addon>`), or the profile's database lacks rows the career uses (`Missing rows from <addon>`).
  - `Made with another version of <addon>`: the addon is on, at a different version from the one the career was saved with.
- A ready row loads the career.
- A row that is not ready opens **Add-ons do not match** with **Load Anyway** and **Back**. **Load Anyway** loads the career. **Back** returns to the list.
- **Add-ons do not match** names the addons that are off or not installed, and each addon the career was saved with at another version as `<addon> (saved with <version>, installed <version>)`.
- A `Missing rows from <addon>` row does not load: the database lacks the career's own team, car or driver. Pressing it shows **Career can't load** on the list. Turn on the addon build the career was made with to load it.
- A career of an addon's own career mode (Progressive, F1 1990) is listed when that addon is off or not installed, as `Missing: <addon>`. It does not load: pressing it shows **Career can't load**. Turn the addon on to load it.
- When the list can't open a row's career, the game's Resume Career screen stays open with **Career can't load**. Pick the career on that screen. `loader.log` names the reason on its `[career] resume screen: row <n>: not pressed` line.
- **New Career** opens the game's Career menu.
- With no careers, the list shows **New Career** and `No careers yet`.

To use the game's own Career menu and Resume Career screens instead, start the game with `SWOFTY_RESUME_LIST=0` in its environment.

## Careers saved under earlier addon ids

When an addon's team, driver or car rows get new ids, the build writes `swofty/id_aliases.json` with the earlier ids mapped to the current ones. A career saved under the earlier ids loads with its teams, drivers and cars.

- At **Play** the launcher maps such a career to the current ids once, after the vault snapshot it takes before Play.
- With a plain copy of the career, it writes a new plain copy with the current ids, and the core puts it in place when the game loads the save.
- With no plain copy yet, or one older than the save, it maps the launcher's save file itself.
- A career is mapped only in a mode whose saved teams use the earlier ids. A save the game writes after that is not mapped again.
- A save file it cannot read or write is left as it is and tried again at the next **Play**.

```text
$ swofty saves remap-ids --plain <profile saves>/plain --aliases <game>/swofty/id_aliases.json --dry-run
```

`--dry-run` reports what would be mapped in each career and writes nothing.

## Versions

Each version is one line: why it was kept (Before Play, After Play, Imported from Steam, Copied from another profile, Launcher start, Changed outside the launcher, Before restore, Restored, Old backup), how long ago (hover for the date), its size, **Pin** and **Restore**.

A restore keeps the save it replaced as **Before restore**, and the careers list shows the restored version's careers until your next Play.

**Pin** keeps a version; retention never removes it. **Unpin** lets retention have it again.

**Restore** opens a panel before anything changes:

```text
You get back   Progressive 2016, Verstappen, Red Bull (season 2016, round 7)
You lose       Classic 2020 (not in that version)
Also           Settings and setups go back to 2026-10-30 22:15 UTC. The save as it is now is kept as a version.
```

Restore waits until F1 25 is closed, and for the Vanilla save until Steam is closed too, as in [Restoring a profile save](/using/profiles-and-restoring#restoring-a-profile-save).

## Using your Steam saves instead

The arrow beside **Play** opens a menu. **Use launcher saves** (ticked by default, absent for Vanilla) turns the profile's own save on or off; with it off the profile plays on your Steam save like Vanilla does. While it is on, **Play once with your Steam saves** runs one session on the Steam save and leaves the setting on:

```text
$ swofty run --steam-saves-once
```

If the core can't use launcher saves it closes F1 25 before the title screen, and the play bar says `Launcher saves could not be used, so F1 25 closed before loading a save`, with the same **Play once with your Steam saves**, **Open logs** and **Dismiss**. Nothing was read or written. The launcher has a save backend for the EA app copy of F1 25. EA app copies of 1.0.145 still run without add-ons.

## Which save Play will use

A line on the play bar names the save the next **Play** uses. With launcher saves in use it reads `Saves: grid launcher save`, with the profile's name. When Play uses the game copy's own save instead, the line turns amber and reads `Saves: Steam's own save (your launcher careers are not used on this run)`, with the reason under it. On the EA app copy it says `the EA app's own save`.

| Reason line | Case |
|---|---|
| `Vanilla profile` | The active profile is Vanilla. |
| `launcher saves are off` | **Use launcher saves** is unticked for the profile. |
| `this game version is not supported yet` | The game's `F1_25.exe` is a build the launcher doesn't know. This holds for every profile. |
| `you chose to play once with your Steam saves` | **Play once with your Steam saves** is starting. |
| `launcher saves are not available on the EA app copy` | The active copy is from the EA app. |

The line is read-only, and `swofty run` makes the same decision. Your launcher careers stay in `saves/<profile>/remote` and are not changed by a run on the Steam save.

## The first time

The first launcher start of 1.7.0 copies your Steam save into every profile's save and opens one dialog:

```text
Your careers have their own saves now
Each profile now has its own save, starting from a copy of your Steam save. Playing from Steam still uses your Steam save, and Vanilla plays on it too.
[Show me]  [OK]
```

**Show me** opens Saves. The dialog is recorded as seen in `saves/imported.json` (`"seen": true`) and never shows again.

## When careers go missing

A banner at the top of the page, and on the **Addons** page, names the missing careers, the profile whose save they went missing from, and the time they were last in the save. **Restore** asks the game to put them back the next time you press Play. **Review the restore** opens that profile's versions with the panel for that version. **Dismiss** hides the banner until the next career goes missing. A career counts as missing when the last save a launcher session wrote has it and the save the game read since does not; the size of the save file is not looked at.

A save that keeps F1 25 on Loading... after Press any button gets a banner with **Restore** of the last version that loaded and **Dismiss**. The launcher changes nothing until you choose.

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

Before the core has written a career index, the commands say so and list the versions without careers:

```text
$ swofty saves careers
no careers yet: the vault learns them from the career index F1 25 writes in a launcher session

$ swofty saves versions
1 version(s) in /home/you/.local/share/swofty/career-vault/steam-843502855, 0.4 MB of 500.0 MB
Steam Cloud now: unknown
20261031-153032-manual  backup, profile default, 0.4 MB, Steam Cloud: unknown, careers unknown, pinned (backup)

$ swofty careers
no careers in the index yet: the core lists each career it creates or loads
```

`swofty saves backup`, `restore`, `import` and `export` act on the active profile; pass `--profile <name>` to `swofty` or to `import` and `export` for another. `--source launcher-<profile>` points `list`, `show`, `versions`, `careers`, `pin`, `unpin` and `restore` at a profile's launcher saves instead of the Steam save:

```text
$ swofty saves list --source launcher-default
vault /home/you/.local/share/swofty/career-vault/launcher-default: 1 snapshot(s), 0.4 MB of 500.0 MB
save /home/you/.local/share/swofty/saves/default/remote: 2 file(s), 400416 bytes, Steam Cloud: local only (no Steam Cloud)
20261031-153037-imported  Imported from Steam saves, profile default, 2 file(s), 400416 bytes
```

See [`swofty saves`](/reference/cli#swofty-saves) for every flag and [Saves and the career vault](/reference/saves) for the folders.
