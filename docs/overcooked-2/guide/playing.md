---
title: Playing with the launcher
description: Pick Overcooked 2 in the bottom-left of the launcher, install and turn on addons in the Library, find addons in the Addon Browser, and press Play to start the game with Swofty Core.
---

# Playing with the launcher

## Picking the game

The **Game** picker is in the bottom-left of the launcher, on the game art. Select it and pick **Overcooked 2**. Each game shows **Found**, **Found · 2 copies** or **Not found**.

With Overcooked 2 picked, the launcher has four tabs: **Home** with the news, **Library** with your Overcooked 2 addons, **Discover** with the Overcooked 2 Addon Browser, and **Donate**. **Online** and the F1 25 pages, such as Shaders, the DB Editor and Saves, show with F1 25 picked. The launcher remembers the game you picked.

Next to **Play**, the play bar shows the copy of Overcooked 2 it plays: its Steam build, **STEAM** or **EPIC**, and its folder. With more than one copy, select it and pick **Play this copy** on another.

## Library

**Library** lists the addons installed for Overcooked 2, in the order they load at Play.

- **Install addon…**: **Choose a file…** for a zip, **Choose a folder…** for an addon folder. Dropping either on the launcher does the same. It needs a `swofty.json` whose `compat.game` is `"overcooked_2"`. An F1 25 addon is refused.
- **On and off**: the switch on each addon. Only addons that are on load at Play.
- **Open folder**: in the header, `$DATA/games/overcooked_2/addons/`; in an addon's menu, that addon's folder.
- **Uninstall**, in an addon's menu: removes the addon's folder. Its data folder, `addon-data/<id>/`, stays.

The command line does the same:

```sh
swofty --game overcooked_2 addons list
swofty --game overcooked_2 addons install ./hello-kitchen.zip    # installs it and turns it on
swofty --game overcooked_2 addons disable swofty.hello-kitchen
swofty --game overcooked_2 addons enable swofty.hello-kitchen
swofty --game overcooked_2 addons remove swofty.hello-kitchen
swofty --game overcooked_2 play --wait                            # waits for the game to close, then prints status.json
swofty games                                                     # every game, whether it is supported, where it was found
```

## Addon Browser

**Discover** is the Addon Browser for Overcooked 2: the same browser as F1 25, listing the addons published for Overcooked 2. **Install** puts an addon in the Library and turns it on.

## Play

**Play** writes the session file and starts Overcooked 2 with Swofty Core. Steam must be running. While the game runs, the play bar shows **Running · 2 addons on · pid 41234** and **Stop**, which closes the game. After it closes, the play bar shows **2 addons on · 1 failed to load last time** when an addon failed, and **Core log** opens that session's `core.log`.

On Linux, the launcher plays the native Linux build. The Windows build under Proton is not supported yet.

In the game, the main menu has a **MODS** tab after **SETTINGS**, with **ADDONS** and the buttons your addons add. **ADDONS** opens a dialog with Swofty Core's version, the game's build and every addon that is on:

```text
Addons

Swofty Core 0.1.0 (std 0.1.0)
Overcooked 2 build 66.678012

Hello Kitchen 0.1.0: loaded
Party Mode 1.2.0: failed
```

The session's `core.log` has what Swofty Core did and the error of a failed addon: see [Logs](/overcooked-2/core#logs).

## Where the files are

```text
$DATA/games/overcooked_2/
├── addons/<id>/           installed addons
├── enabled.json           {"enabled": ["swofty.hello-kitchen"]}
├── addon-data/<id>/       each addon's own folder
├── sessions/<stamp>/      session.json, core.log, status.json, player.log (Unity's log), output.log
├── catalog.json           the Addon Browser's cache
└── downloads/
```

`$DATA` is the launcher's data folder: see [Where your files are stored](/guide/installation#where-your-files-are-stored).
