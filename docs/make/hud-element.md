---
title: Build a HUD element
description: Put your own dial in the race HUD with PNG sprites, a hud.element record, live state reads and leaves written from native code, and test it on the host without the game.
---

# Build a HUD element

You'll build a small dial for the 2026 car's HUD: a speed text, a gear text, an RPM sweep, an ERS bar and a pit lamp. The speed, the RPM and the ERS bar come from the game's own HUD values. The gear and the pit lamp are leaves your plugin writes from the live state. The finished addon is [`templates/hud-dial`](https://github.com/Swofty-Developments/SwoftyLauncher/tree/main/templates/hud-dial).

A HUD element has three parts:

```text
hud.element record + PNG sprites    build time    what the dial looks like and which value drives each part
swofty/live.hpp                     runtime       the cars, session, classification and gaps, read from the game
swofty/hud_leaves.hpp               runtime       your own values, published as typed leaves the record binds
```

## The folder

```text
hud-dial/
├── swofty.json
├── registry/hud.json            the hud.element record
├── art/                         base.png ring.png solid.png fill.png lamp.png
├── tools/make_art.py            writes the five PNGs, standard library only
└── native/
    ├── src/dial.hpp             the leaves and the per-frame logic
    ├── src/addon.cpp            hooks the frame callback
    ├── test/dial_test.cpp       host test over fake::Live and fake::Hud
    ├── test/run_test.sh
    └── build.sh                 zig c++ to native/hud_dial.dll
```

```json [swofty.json]
{
  "schema_version": 1,
  "id": "example.hud-dial",
  "name": "HUD Dial",
  "version": "0.1.0",
  "kind": ["ui", "native"],
  "compat": { "game": "f1_25", "game_builds": [">=25251167"] },
  "requires": [{ "id": "swofty.core", "version": ">=0.1.0" }],
  "experimental_features": ["telemetry"],
  "operations": {
    "hud": ["registry/hud.json"],
    "native": [{ "dll": "native/hud_dial.dll" }]
  }
}
```

`telemetry` is the feature that turns the live state on. An addon that draws only game values, with no `native` entry, needs neither it nor a DLL.

## The record

Sprites are PNGs whose sizes are multiples of 4, drawn at 3 pixels per HUD unit. `bounds` is where each sprite sits in the 640x360 HUD frame, with the origin at the dial centre and `v` pointing down. The record binds each part to a source:

```json [registry/hud.json]
{
  "hud.element": {
    "dial": {
      "dial": "f1_26",
      "stock": "park",
      "sprites": {
        "base":  { "png": "art/base.png",  "bounds": [-60, -36, 60, 36] },
        "ring":  { "png": "art/ring.png",  "bounds": [-24, -24, 24, 24] },
        "solid": { "png": "art/solid.png", "bounds": [-4, -4, 4, 4] },
        "fill":  { "png": "art/fill.png",  "bounds": [-40, 24, 40, 32] },
        "lamp":  { "png": "art/lamp.png",  "bounds": [-56, -30, -40, -22] }
      },
      "elements": [
        { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
        { "kind": "text", "id": "gear", "style": "17_bold", "align": "centre", "box": 12,
          "cap_height": 6, "at": [0, -6], "value": { "view_model": "view_model_data.hud_dial_gear" } },
        { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 30,
          "cap_height": 9.5, "at": [0, 10], "value": { "game": "speed_str" } },
        { "kind": "sweep", "id": "rpm", "mask_sprite": "ring", "fill_sprite": "solid",
          "pivot": [0, 0], "start": 225, "arc": [[0, 0], [1, -270]], "value": { "game": "revs" } },
        { "kind": "gauge", "id": "ers", "sprite": "fill",
          "path": [[-40, 28], [40, 28]], "value": { "game": "ers_stored" } },
        { "kind": "sprite", "id": "pit", "sprite": "lamp",
          "visible": { "view_model": "view_model_data.hud_dial_pit" } }
      ],
      "editor": {
        "preview": { "game.revs": 0.62, "game.speed_str": "212", "game.ers_stored": 0.75,
                     "view_model_data.hud_dial_gear": "6", "view_model_data.hud_dial_pit": true },
        "team_colour": "#27F4D2"
      },
      "variants": {
        "f2": { "dial": "f2", "remove": ["ers"] }
      }
    }
  }
}
```

- `"game": "speed_str"`, `"revs"` and `"ers_stored"` are values of the dial's own view model. The build checks each against the tags the stock template binds.
- `"view_model": "view_model_data.hud_dial_gear"` is a leaf your plugin writes. A text binds a text leaf, `visible` binds a bool leaf. The build refuses a leaf bound with another type than the one it declares.
- `"stock": "park"` moves the stock frame, fuel and pit parts off screen. The F2 variant removes the ERS bar, because an F2 car has no ERS.
- `editor.preview` is what the HUD editor shows, since the editor runs no template.

The full record format, element kinds, decisions and the rules the composer keeps are in [HUD elements](/reference/ops/hud).

## The sprites

`tools/make_art.py` writes the five PNGs with `zlib` and `struct`, so there is nothing to install:

```sh
python3 tools/make_art.py
```

Replace them with your own art at the same sizes, or at any size that is a multiple of 4.

## The leaves

`swofty/hud_leaves.hpp` declares each leaf once with its type. The core keeps the last value written, sends only changes, and refills the leaves after the game rebuilds its view model, so the plugin has no refresh timer.

```cpp [native/src/dial.hpp]
#pragma once
#include <optional>
#include <swofty/hud_format.hpp>
#include <swofty/hud_frame.hpp>
#include <swofty/hud_leaves.hpp>
#include <swofty/live.hpp>
#include <swofty/mixin.hpp>

namespace dial {

namespace hud = swofty::hud;
namespace live = swofty::live;

struct Leaves {
    hud::Leaves L{"view_model_data.hud_dial_"};
    hud::TextLeaf gear = L.text("gear", 2);
    hud::BoolLeaf pit = L.boolean("pit");
};

inline const char *gear_text(int gear, char out[2])
{
    if (gear < 0)
        return "R";
    if (gear == 0)
        return "N";
    out[0] = static_cast<char>('0' + (gear > 9 ? 9 : gear));
    out[1] = 0;
    return out;
}

class Dial {
public:
    void frame(double dt)
    {
        std::optional<live::Car> me = live::available() ? live::subject() : std::nullopt;
        std::optional<live::Session> session = live::session();

        hud::Gate gate;
        gate.live = me.has_value() && session.has_value();
        if (session) {
            gate.session_serial = session->generation;
            gate.subject_serial = session->subject_generation;
            gate.time_trial = hud::format::is_time_trial(session->type);
        }

        if (!frame_.begin(gate, dt)) {
            blank();
            return;
        }

        char digit[2];
        if (live::has(*me, SWOFTY_LIVE_HAVE_GEAR))
            leaves_.L.set(leaves_.gear, gear_text(me->gear, digit));
        leaves_.L.set(leaves_.pit, live::in_pit(*me));
        leaves_.L.flush();

        if (frame_.end()) {
            auto cost = frame_.cost();
            swofty::log("[hud-dial] frame %.0f us mean, %.0f us max over %d frames", cost.mean_us, cost.max_us, cost.frames);
        }
    }

private:
    void blank()
    {
        leaves_.L.set(leaves_.gear, "");
        leaves_.L.set(leaves_.pit, false);
        leaves_.L.flush();
    }

    Leaves leaves_;
    hud::Frame frame_;
};

}
```

```cpp [native/src/addon.cpp]
#include <swofty/events.hpp>
#include <swofty/mixin.hpp>

#include "dial.hpp"

namespace {

dial::Dial the_dial;

void on_frame(double dt, void *)
{
    the_dial.frame(dt);
}

}

SWOFTY_ON_LOAD
{
    swofty::events::on_frame(on_frame);
}

SWOFTY_ADDON()
```

- `live::subject()` is the watched car while the spectator HUD is up, else the player's. `hud::Frame` resets on its serial, so a spectator switch is one `changed_subject()` call away.
- `live::has(*me, SWOFTY_LIVE_HAVE_GEAR)` says the gear was read this frame. A clear bit means the field is zero, so the dial leaves the last text instead of drawing a wrong gear.
- `Frame::end()` is true every 10 seconds and reports the frame's mean and worst cost.

See [Live state](/reference/std/live), [HUD leaves](/reference/std/hud-leaves) and [HUD frame](/reference/std/hud-frame) for the other reads: gaps, the classification, turn numbers, rows, one-hot and level groups, cadences.

## Test it without the game

`fake::Live` answers `swofty::live::*` from a car you build, and `fake::Hud` records every leaf the core would get, with the core's own rules, so a type mismatch or a write outside the HUD prefixes fails the test.

```cpp [native/test/dial_test.cpp]
#include <cassert>
#include <swofty/fake_hud.hpp>
#include <swofty/fake_live.hpp>

#include "../src/dial.hpp"

int main()
{
    swofty::fake::Hud core;
    swofty::fake::Clock clock;
    swofty::fake::Live fake;
    dial::Dial dial;

    fake.session().type = 15;
    auto &car = fake.add_car(0, 1);
    car.gear = 6;
    car.have |= SWOFTY_LIVE_HAVE_GEAR;
    fake.set_order({0});
    fake.set_player(0);
    fake.install();

    dial.frame(0.016);
    assert(core.text("view_model_data.hud_dial_gear") == "6");

    core.clear();
    dial.frame(0.016);
    assert(core.writes().empty());          // unchanged leaves are not rewritten

    fake.set_available(false);              // no session: the dial blanks
    dial.frame(0.016);
    assert(core.text("view_model_data.hud_dial_gear").empty());
}
```

```sh
native/test/run_test.sh
```

```text
hud-dial test: ok
```

## Build, check and install

```sh
native/build.sh                           # native/hud_dial.dll
python3 tools/std/validate.py --data --addon example.hud-dial registry/hud.json
swofty hud check                          # the stock dial facts the composer relies on
swofty hud compose . registry/hud.json --id example.hud-dial --out hud-out
swofty addons install .
swofty addons enable example.hud-dial
swofty run --apply
```

`swofty hud compose` writes the same overlay archives the build writes, so you can read what the composer made. `swofty hud check` reads the game's original files and needs a game folder.

## What is experimental

::: warning Not seen in game yet
The HUD composer, `SwoftyHud` version 2, the leaf table and the memory-backed live state are covered by host tests, by builds against the stock dial scenes and by round trips of the written files. None of them has run in the game yet. Where each part lands on the dial is the first thing to check.
:::

- **Live state is experimental.** It needs `"experimental_features": ["telemetry"]`, and its calls answer empty without a session.
- **Live groups are verified one by one.** A group of the live state reads only when every site it needs resolves on the running build and its rows in the knowledge base are verified. Tyres, game mode, team badges, the track name, the spectator gap mode, the pit-lane flags behind `pit_status` and `pit_stops`, and the DRS bit are off until their rows land. They read under `SWOFTY_LIVE_FORCE=1` on a test machine. The core logs each group it leaves off once.
- **Not read at all yet:** active aero mode, Boost and Overtake state, DRS allowed, the pit limiter, driver name and team colour, the session clock and lap total, lap and sector times, tyre temperatures. Their `have` bits stay clear.
- **The EA build** has no sites for the live state until its bindings exist, so the calls answer empty there.
- **`gear` here is a leaf of your own.** The game's own HUD values the record can bind are `speed_str`, `revs`, `ers_stored`, `om_active`, `is_xm_active`, `has_ers`, `ers_mode` and `drs_pit_state`.

The live state's [readiness section](/reference/std/live#when-a-group-reads) lists the groups and what turns each on.

::: info Credit
The element templates, the placement maths and the leaf helpers follow Project Halo by nyannoying (https://github.com/nyannoying1337/f1-25-project-halo). Uses work from Project Halo by nyannoying.
:::
