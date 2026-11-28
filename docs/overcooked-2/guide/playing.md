---
title: Playing with the launcher
description: Pick Overcooked 2 in the bottom-left of the launcher, install and turn on addons in the Library, find addons in the Addon Browser, and press Play to start the game with Swofty Core.
---

# Playing with the launcher

## Picking the game

The game picker is in the bottom-left of the launcher, on the game art. Select it and pick **Overcooked 2**. Each game shows whether the launcher found it.

With Overcooked 2 picked, the launcher has three tabs: **Home** with the news, **Library** with your Overcooked 2 addons, and **Discover** with the Overcooked 2 Addon Browser. F1 25's pages stay with F1 25. Pick **F1 25** to get them back. The launcher remembers the game you picked.

The play bar shows the Overcooked 2 copy it found: the store, the build and the folder. With more than one copy, pick the one to play there.

## Library

**Library** lists the addons installed for Overcooked 2.

- **Install**: select **Install from file** and pick a zip or a folder, or drop one on the page. It needs a `swofty.json` whose `compat.game` is `"overcooked_2"`. An F1 25 addon is refused.
- **Turn on and off**: the switch on each addon. Only addons that are on load at Play.
- **Open folder** opens the addon's folder in `$DATA/games/overcooked_2/addons/<id>/`.
- **Uninstall** removes the addon and its folder.

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

**Play** writes the session file, starts Overcooked 2 with Swofty Core and shows **Running**. Steam must be running. **Stop** closes the game.

In the game, the main menu has an **Addons** button. It opens a dialog with Swofty Core's version and every addon with its state: loaded, or failed with the error.

When something goes wrong, the session's `core.log` says what Swofty Core did: see [Logs](/overcooked-2/core#logs). [Send your logs](/using/send-logs) to the developers with the launcher.

## Where the files are

```text
$DATA/games/overcooked_2/
├── addons/<id>/           installed addons
├── enabled.json           {"enabled": ["swofty.hello-kitchen"]}
├── addon-data/<id>/       each addon's own folder
├── sessions/<stamp>/      session.json, core.log, status.json
├── catalog.json           the Addon Browser's cache
└── downloads/
```

`$DATA` is the launcher's data folder: see [Where your files are stored](/guide/installation#where-your-files-are-stored).
