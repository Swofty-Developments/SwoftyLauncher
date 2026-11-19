# F1 25 UI system — authored formats + runtime model (edit/add reference)

Owner lane: `ui` (prefix `Ui`; owner in `subsystems.tsv`). Companion map: [`ui.md`](ui.md). The
engine side of the same graph (template store, instantiation, behaviours, signals, failure modes) is the
`vm` lane map [`vm.md`](vm.md); the user-facing pages are `docs/reference/ui/{engine,nodes,patterns}.md`.
Exe-side functions/globals live in the `vm`, `fe_flow`, `fe_home`, `fe_tiles`, `fe_popup`,
`fe_input`, `hud`, `assets` and `career` lanes — this doc is the **format and workflow**
reference that ties them together, so a future addon can edit and add UI in a general way.

Distilled from, and kept honest against, three primary sources (all in
`/mnt/work/VSC/F1ModManager`):
- the launcher's UI generator — `packages/core/src/games/f1_25/ui/{node,pages,patch,check,mod}.rs` (it
  authors working `.ui`/UIDF screens and patches the stock UI ERPs, so it is the Rosetta stone);
- the binary format crates — `packages/formats/src/{pkg,erp}.rs`;
- the native runtime loader + view-model binder — `native/src/core/frontend/{ui,ui_vm}.cpp`;
- since 2026-10-23, the engine's own loader read from `F1_25.exe` (`vm.md#templates` .. `#failures`,
  `re/db/ui/loader_rules.json`) and the stock catalogue `tools/uicatalog/catalog.json` (345 `.ui`,
  5,208 `.idf`).

Engine facts below carry a marker: **[L2]** read in the decompile with callers listed (the KB row is
L2), **[run]** also seen in the 2026-10-23 live gdb trace (`vm.md#trace`), **[game]** observed in game
without a trace, **[L1]** scan or rules-file only, **[inferred]** not confirmed. Facts without a marker
are format facts from the launcher's own reader and writer.

Image base `0x140000000`, build `d873a1c8…` (Steam), code section `.xtls`.

---

## 0. The stack at a glance

EA's EGO/`neon` UI is a declarative scene graph serialized in a tagged binary container. Five
layers, each editable by a different route:

| Layer | On disk | What it is | Edit route |
|---|---|---|---|
| Container | `!pkg` | tagged binary blob (objects/lists/value-tables) wrapping everything below | `pkg.rs` / `tools/pkg.py` |
| Screen graph | `.ui` (UIDF) | named node tree: widgets, props, data bindings, events, layout attach | author nodes (§2) |
| UI data | `.uidb` | variables + dictionaries the menus read/write; input-prompt tooltips | §4 |
| Routing | `.uir` + `flows.ui` + `region_loading.uiconfig` | which screens open/close/transition, and which flow/ERP loads them | §4 |
| Layout/art | `.idf` (scene) | authored rects/positions/anchors a screen attaches to by content-hash | `!pkg` `eScene` graph; `idf.py` (§5) |
| Packaging | `.erp` (ERPK) | compressed archive holding the resources above, addressed by `eaid://` | §6 |
| Live tree | — | the VM node tree the engine builds from the authored graph + bound data | §7 |

Two independent *delivery* routes reach the running game (§8):
- **Authored-asset route** — rewrite/patch the stock ERPs (what `swofty build` does for ADDON
  SETTINGS, and `operations.erp` / the planned `operations.ui` do for addons).
- **Runtime route** — a native plugin registers whole new `.ui` screens through the engine's
  own loader (`operations.screens`), and/or builds/edits the live VM node tree directly in a
  mixin (how the stock R&D boxes/pie are built, and how a 5th one must be added).

---

## 1. The `!pkg` container (binary) {#pkg}

Every `.ui`, `.uidb`, `.uir`, `.uiconfig`, `.uimsg`, `.layer` resource is a `!pkg`. Format in
`packages/formats/src/pkg.rs`; mirror in `tools/pkg.py`.

### Header
```
+0x00  "!pkg"            magic
+0x04  name[4]           Tag (often zero for UI)
+0x08  root_ref  u32     reference to the root node
+0x0c  vca_ref   u32     reference to the "!vca" value-table block
```
A **reference** is `(tag<<24) | (offset & 0x00ff_ffff)`. The top byte selects where the value
lives: `0x80` (`CHUNK`) = a chunk at that file offset; any other value `t` = scalar from value
table `t`, `offset` interpreted inside that table. On write, resolved pointer slots carry the
`0x8000_0000` bit; offsets are always masked with `0x00ff_ffff` (24-bit — a single `!pkg` is
capped at 16 MB of chunk space).

### Chunks (nodes)
- `!idi` **object**: `u32 = count | (version<<24)`. Entry stride is **12 bytes when
  version==1, else 8**: `key_ref` @+0, `value_ref` @+4, and (v1 only) `fnv1a32(key)` @+8.
  v1 object keys are stored **sorted by `fnv1a32(key)`** (the writer sorts; readers may binary
  search). `Obj` is an ordered `Vec<(key, value)>` in memory.
- `!ili` / `!vca` **list**: `u32 count` then `count` refs. In-memory `Arr` carries its tag
  (default `!ili`).
- `!iar` **inline raw**: 8 literal bytes (used for opaque 8-byte blobs).

### Value tables (`!vca` block)
```
"!vca"  u32 count   then count refs to tables
  !sbi  u32 len,   len bytes      string blob: pascal strings (1 length byte + bytes + NUL)
  !vbi  ty[4] u32 count u32 bpd, count*bpd bytes   typed scalars
```
A scalar ref `(t<<24)|off`: for a string table `off` is the byte offset of the pascal string;
for a typed table `off` is the element index. Tables are grouped by type and **aligned by
type**: 16 B for `vec3/vec4/quat/woid/shnm/mat4`, 32 B for `bbox`, 8 B for `ui64/si64`, 4 B
otherwise. Scalar `ty` tags seen: `stri ui32 si32 ui16 ui08 bool fp32 ui64 si64 vec3 vec4
quat mat4 bbox woid shnm`.

Writing rule (round-trip): unchanged resources must write back **byte-identical**. The writer
reuses a value's original table slot when the bytes still match, interns new values, keeps key
order (re-sorting only v1 hashes), and pads to alignment. `pkg.rs` tests assert this; keep it.

---

## 2. The `.ui` / UIDF node grammar {#ui}

A `.ui` file's root is an object `{ "Screens": [ <screen-node>, ... ] }`
(`node.rs::ui_bytes`). Everything inside is a **node**. A `.ui` resource has up to two fragments the
engine reads: `temp` (fragment 0: `Screens`, `Flows`), built later by a screen job, and `syst`
(fragment 1: `Templates`), registered synchronously by name (§2.9) **[L2]**.

### 2.1 A node is an 8-element list
`is_node(n)` ⇔ `n` is a list of length 8 whose item 0 is an object with a `name` key
(`node.rs::is_node`). The slots (authoritative mapping from `node.rs`):

| Slot | const | Holds |
|---|---|---|
| 0 | `PROPS` | properties object (always has `id version type nameId name`, then widget props) |
| 1 | `CHILDREN` | list of child nodes (and inline data-decision nodes) |
| 2 | `COMMANDS` | list of command handlers `{commandName, command, signals[]}` |
| 3 | `EVENTS` | list of event handlers `{eventName, event, signals[]}` |
| 4 | `LAYOUT` | `[path]`: the layout path is the first entry, a list of steps (§2.5). The engine reads only `slot4[0]`; an empty path fails the node with 0x22; an empty slot 4 means no attach **[L2]** |
| 5 | `COMPONENTS` | scene components this node instantiates: `[{componentId: ui64 = md5-64(path), debugName: <eaid path without .idf>, name}]` (e.g. `tab_content_rnd_upgrades_alt` → `rnd_overview`, the UITH `laptop_r_and_d_tab_header` → `primary_icon_nav_item`); its descendants then attach by name into that scene. The engine reads the first entry, which needs `name` and `componentId` (else 3; `debugName` is not read), and adds it as one more layout step **[L2]** |
| 6 | `BEHAVIOURS` | list of `{type: <FourCC>}` neon behaviours; `type` is required (else 3); some types require props (§2.6) **[L2]** |
| 7 | — | empty in authored screens (role unread) |

The launcher's `is_node` wants 8 slots; the engine (`Vm_Read_UidfNode` 0x14417f5f0) accepts a node
array whose slots 6 and 7 are absent, needs every present slot to be a readable `!pkg` array/object and
the props to hold `name` and `type`, else 5 **[L2]**. It reads the record into name `char*` +0x68,
FourCC +0x70, object +0x78, children +0x10.

### 2.2 The props object
Every node's props carry five fixed keys (`node.rs::node`), then widget-specific props:
```
id      ui32   = djb2(full dotted path, e.g. "swofty_settings.col_0.t0")
version ui32   (0)
type    ui32   = the class FourCC (little-endian; see §3)
nameId  ui32   = djb2(name)
name    stri   the node's local name
```
`id`/`nameId`/layout `nodeId`/event+command/signal hashes are all recomputed by
`node.rs::fix_ids` after any edit — **always re-run the id fixup after renaming or moving a
node**, or the engine's hash lookups miss. The element init compares `type` with the class's own FourCC
(vt+0x80) and fails the node with 5 on a mismatch **[L2]**. Props are found by a case-insensitive string
compare of the key names (`Vm_Find_PkgObjectKey_Thunk` 0x143927f90), not by their stored fnv1a32; a
missing required key returns 3 **[L2]**.

### 2.3 Property values and data binding (`@`) {#binding}
A property value is either a bare scalar, or a **binding object** `{ value, dbentry }`
(`node.rs::prop`). `dbentry` is the bound path; `value` is the fallback/literal. `tools/uitree.py`
prints a binding as `key=<value>@<dbentry>`.

Binding path roots (resolved by `Vm_Resolve_RegistryNode`, vm 0x1442b1b60):
- `#ui_menu_data.<…>` — absolute path into the `ui_menu_data` dictionary of the global VM
  registry `g_Vm_Registry` (0x146ca54f8). The `#` prefix means "absolute"; `node.rs::strip_data`
  keeps `#`-prefixed dbentries and drops purely local ones when cloning a template.
- `view_model_data.<screen>.<…>` — the per-screen view-model root. The native binder sets a
  VM's `path_ = "view_model_data." + screen` (`ui_vm.cpp`), so a leaf `o0.value` resolves under
  `view_model_data.<screen>.o0.value`.
- relative leaf (`label`, `o3.text`, `s2`) — resolved against the node's data/VM node.
- `network.<…>`, `user_attributes.<…>` — other registry roots (session state, profile flags).

**Labels / text** (`UILB`): props `{ text: { value, dbentry, isloctag }, localise }`.
`localise=true` + `isloctag` → `value`/`dbentry` is an `lng_*` language key looked up in the
language files. For literal text, `node.rs::literal_label`/`literal_text` set the string into
`value`, drop `dbentry`+`isloctag`, and set `localise=false`. Inline style markup inside a label
string: `{s:<style>}TEXT{/s}` (`pages.rs::tagged`, e.g. style `18_regular_upper_part_neg`).

**List controls / the tile-spawner (`@rows`)** — classes `UITG UITB UILT UIL2 UIEL`
(`check.rs::LIST_CLASSES`, matched by FourCC in `ui.cpp::list_base`). These MUST carry
`data = { value, dbentry }` with a **non-empty `dbentry` that points at an array**. The engine
then spawns **one child instance per array element** — this is the stock pattern for upgrade
tiles, pills, result rows, etc. Every shipping `UITG` also binds `selection = @<value path>`.
If `data` is missing, literal, or has an empty `dbentry`, the node init fails and **the whole
screen fails to open** (`ui.cpp::init_reason`: list-base code 3/5). ADDON SETTINGS sidesteps
this by authoring one explicit row node per option instead of a spawner (`pages.rs::build_page`).

### 2.4 Events, commands, signals
`EVENTS`/`COMMANDS` entries are objects (`node.rs::event`/`command`):
```
eventName / commandName   stri
event / command           ui32 = djb2(name)
signals[]                 list of signal objects
```
A **signal** (`node.rs::sig_with`) wires one node's output pin to another node's input pin:
```
to_object       ui32 = djb2(to_object_name)
to_object_name  stri   target node's full dotted path
to_pin_name     stri   target input pin
to_pin          ui32 = djb2(to_pin_name)
from_pin_name   stri   this node's output pin / the event name
attributes?     obj    typed args passed with the signal
```
Lifecycle/nav pins seen: `focus navUp navDown navLeft navRight pressed focused route reset
stop show setText trigger query is_valid`.

**What the loader requires** (`Vm_Read_CommandList` 0x1442c29e0 / `Vm_Read_EventList` 0x1442c2fd0 →
`Vm_Read_SignalList` 0x1442c17e0) **[L2]**: every command entry needs `command` and `signals`, every
event entry `event` and `signals` (an empty `signals` array is accepted); every signal needs `to_pin` and
`to_object`; `attributes` are optional; each `mappings` entry needs `value`. A miss is code 3 and fails
the node. `commandName`, `eventName`, `to_object_name`, `to_pin_name`, `from_pin_name` and `debugName`
are **never read**: only the hashes count, so a stale hash with a correct name is a broken signal.

**At run time** (`vm.md#signals`) **[L2]**: a signal holds a link handle (`structs/UiSignal.h`), filled
when its target id is found, or left **pending** until a node with that id is created; signals fired
before then are dropped. Inside a template instance, `to_object` ids that name a node of the same
template are rewritten to the instance's runtime ids; ids outside it stay as authored. Raising an event
the node does not declare, or one with no signals, does nothing; a pin the target class does not handle
is ignored. None of these log. A **data-dispatched command** is a signal to the
screen's own node with pin `raise_command` and `attributes.command = {name, data}`
(`node.rs::raise_cmd`) → arrives at `<Screen>VM::onCommandRaised` (slot 10; dispatcher
`Vm_Dispatch_BindingMessage` 0x1442c00e0). This is how a button triggers custom VM logic.

### 2.5 Layout attach (slot 4) — how a node is placed {#layout-attach}
Slot 4 is `[path]`; the engine reads only `path = slot4[0]`, a list of steps, and stores one
`UiLayoutStep` per step in the node's attach behaviour (`structs/UiLayoutStep.h`, `vm.md#layout-attach`).
Multi-step paths are normal: stock nodes use 2 to 6 steps. An empty path is 0x22; a step that has
`isComponentId` must have `componentId` (step kind 1), any other step must have `nodeId` (kind 0), else
3 **[L2]**. Nothing is resolved against the `.idf` at load: the steps resolve when the element attaches
to its scene, so a step naming a missing element fails silently later, not at load (how a step descends
through `eXRef`/`eMask` is not mapped). Two legal forms (`check.rs` rules `layout.path_nonempty` and
`layout.step_keys` reject a path that breaks these):
- **`nodeId` attach** — `{ nodeId: ui32 = djb2(debugName), debugName: stri }`
  (`node.rs::layout_path`). The node drops into a named slot of the **parent's** layout, keyed
  by `djb2(slot debug name)`. This is how the launcher attaches tiles/rows into the stock
  containers' existing slots (e.g. `"1a"`, `title_1`, `stacker_title`). `debugName` is always
  the scene element's **leaf name**, never a dotted path (130,439/130,439 stock attaches), and it
  resolves through the scene's `nodeidlookup` name index (§5.1). A group with several entries
  walks a chain, e.g. `[screen_container, screen_cc]` = the `screen_container` eXRef, then
  `screen_cc` inside the component it references.
- **`componentId` attach** — `{ componentId: ui64 = md5-64(path), isComponentId: bool true,
  debugName: "<eaid-path>.idf" }` (`node.rs::screen`). The **screen root** points at an authored
  scene-layout component (`.idf`, §5) by the **first 8 bytes of `md5(path)` big-endian**
  (`node.rs::component_id`). Example roots the launcher reuses:
  `eaid://ui_package/art/screens/options/options_screen` (home) and `…/options_edit_screen`
  (sub-page).

**Key consequence for the R&D box issue** (`career.md`): the on-screen rects/slots a screen can
lay out are the elements in its `.idf` component (§5). A **fixed-grid** scene exposes exactly the
sibling slots it authored (a 5th needs a new element, §5.7), whereas a **dynamic `eStacker`**
re-lays-out however many children are attached. The two R&D surfaces resolve differently (§5.6):
the R&D > Performance department tiles are the fixed `rnd_overview.idf` grid, while the native
box-grid builder fills a tab controller whose headers stack in an `eStacker`.

### 2.6 Behaviours (slot 6)
A list of `{ type: ui32 = FourCC }` (`node.rs::behaviours`). Two stock sets:
- `SCREEN_BEHAVIOURS` = `NBEL NBDC NBMG NBVM NBAC NBOS NBXA NBMO NBTR` (on `UISC` roots).
- `GROUP_BEHAVIOURS` = `NBEL NBDC NBMG NBVM NBFC NBAN NBAC NBOS NBXA NBMO` (on `UIGP` groups).

The element init creates one behaviour per registered `type` and runs its from-data init (vtable slot
5) in order; a non-zero rc fails the node (`vm.md#behaviours`) **[L2]**. What each init reads:

| type | role | props it reads |
|---|---|---|
| `NBVN` | visual navigation | **`visualNav` object, required** (else 3); then `autoFocus`, `allowXNav`, `allowYNav`, `xOffset`, `yOffset`, `navEnabled` **[L2, run]** |
| `NBMO` | material override | `materialData`, optional **[L1]** |
| `NBEL` | element attach | `hideOnUnbind`, optional **[L1]** |
| `NBAN` | animation | `handleRepeats`, optional **[L1]** |
| `NBAC` | audio context | `audioContext`, optional **[L1]** |
| `NBDC` `NBVM` `NBXA` `NBTP` | — | nothing **[L1]** |
| `NBMG` `NBFC` `NBOS` | — | not registered; the engine skips them **[L1]** |

**`NBVN` needs `visualNav`.** Stock pairs them exactly: every template root with `visualNav` lists
`NBVN` and `f1_world_activity` has neither. A node that keeps `NBVN` in slot 6 and loses `visualNav`
fails with 3; on a template root that takes the whole instance down (§2.9). The era picker edit that
dropped `visualNav` from the `dynamic_era_select` root failed all three era content tabs and emptied the
F1 World play screen; restoring the stock `visualNav = {xOffset: 0, yOffset: 0}` fixed it **[run]**.
`check.rs` rule `behaviour.nbvn_visualnav` refuses such an edit.
Inline data-decision child nodes (classes `SVCD MVCD DBVV DBRV DBCD DBAM WUDN UIVF`,
`node.rs::DECISIONS`) are logic nodes the launcher strips from cloned templates and re-adds
deliberately (e.g. the popup "seen once" gate, `patch.rs::persistent_gate`).

### 2.7 `allowVMBinding`
Only the **screen root** may set `allowVMBinding` (`check.rs` errors otherwise). Set it `true`
to request view-model binding; it must be matched by an actual attached view model (§7) or the
page stays blank and Back breaks (`check.rs::screens` cross-checks against the VM list).

### 2.8 How a screen is composed
`node.rs::screen` builds the root (`UISC`, props copied from a stock screen, layout = the `.idf`
componentId, commands = `open`/`close` route pins, `SCREEN_BEHAVIOURS`). `node.rs::finish` fills
its children in this order: `Action` (`UIAC` route-pin node whose events wire
`opened_screen`/`finishedTranIn`/`finishedTranOut` to focus + a `watchdog` `TIME` node +
tooltip activation), `tooltips` (`UITT` container of `UITL` prompts), `audio_theme_trigger`
(`UIAM`), `back_listener` (`UIKL` key-listener routing `back`), then the content nodes, then the
`watchdog`. A real built page (`uitree.py swofty_addon_5.ui`):
```
swofty_addon_5 [UISC]
  Action [UIAC]  tooltips [UITT](back/adjust UITL)  audio_theme_trigger [UIAM]  back_listener [UIKL]
  screen_title [UIGP](title UILB …)  about [UIGP]  description [UIGP](set…)
  row_2..row_N [UIGP] → toggle/slider [UIGP] → label/value [UILB], left/right_arrow [UIGP], indicator
```

### 2.9 Templates (fragment 1) {#templates}
A `.ui`'s `Templates` list (fragment 1, `syst`) holds reusable roots — usually `UIGP`, each carrying its
own scene in slot 5 — that the game instantiates at run time: tab contents and headers, spawned tiles,
list rows. At load the engine stores only each template's name, `!pkg` buffer and node reference
(`structs/UiTemplateRecord.h`); the tree is built again on every instantiation **[L2]**.

- **Names are global.** The template store spans every loaded `.ui`. A template whose name is already
  registered makes the loader return 0x58, and every later template of that resource is never
  registered; which resource wins depends on load order **[L2]**. `check.rs` rule
  `template.duplicate` refuses a name that repeats in the file or that another stock `.ui` registers.
- **Instantiation.** A caller looks the name up (a miss is skipped silently) and builds the tree under a
  parent; each node attaches to the parent after its own init succeeds and before its children are
  built; the first failing node stops the build and fails the instance **[L2]**. A failing descendant
  leaves the partial tree attached; a failing root attaches nothing **[L2, run]**. No failure logs.
- **Tabs.** A tab controller (`UITC`) gets its tab list from the view model (`tabs[i].content_template`,
  `header_template`) and instantiates every content template, then every header template. Each
  instance is named `tab_<n>( <content_template> )` — **also the header instances** — with `<n>` the
  controller's child count + 1. A failed content instance is dropped and the loop continues **[L2, run]**.
  For `f1_world_activity_container` the seven tabs are `f1_world_activity`, `f1_world_activity_offline`,
  `dynamic_era_select` (three times: Grand Prix, Time Trial, split-screen era select), `public_gp_select`
  and `session_create_dlc_select_tab` (from `f1_world_container.ui`); headers use `primary_nav_tab`
  (`templates/tab_header_templates.ui`). `time_trial_select`, also in that `.ui`, is not used by any tab
  **[L2, run]**.

---

## 3. Node / widget classes (the FourCC registry) {#classes}

Every node's `props.type` is a 4-char class code, little-endian (`"UIGP"` →
`0x50474955`). The class factory table is built by `Vm_Register_NodeClasses`
(vm 0x14419b1d0): ~97 factories, each 0xc0 bytes, installed by `Vm_Install_NodeFactories`
(0x1441cac40); the game registers its own classes (`F1xx`, `TILE`, `LCDC`, `OSPL`, `RSCM`, `TACM`,
`TADM`, `UISS`) in 0x1437ea4a0, 136 in all **[L1]**. `re/db/ui/loader_rules.json` `classes` lists every
class with its vtable, init, pin handler, the props its init reads and the pins its handler implements;
`docs/reference/ui/nodes.md` lists what stock files use per class. When a screen loads, `Vm_Build_UidfNode`/`Vm_Read_UidfNode` look the class up
here; **a FourCC with no factory makes the build fail with code 5** (`ui.cpp::factory_site`:
"has no registered class factory"). Full 97-class list in `vm.md#classes`.

Best-known classes (suffix hint: …L list, …B button/bar, …P panel, …N input/node):
```
UISC screen            UIAC action/route-pin     UIGP group            UIFL flow
UISE route-group       UILB label                UITX text-element     UIBT button
UIKL key-listener      UITG toggle(list)         UITB toggle-bar       UISL slider
UITE text-entry        UILT / UIL2 list          UIEL element-list     SCRL scroll
UITT tooltip-container UITL tooltip              UIDM multichoice-msg  F1FV fullscreen-video
UIAM audio-trigger     UIMO motion/anim          UIXA anim             TIME timer
UIDS data-source       SVCD single-value-decision  MVCD multi-value-decision
DBVV/DBRV/DBCD/DBAM/WUDN/UIVF  data-binding/decision nodes   LOCA localise  SWIP swipe
```
Element-init base chain (`vm.md#element-init`): `Vm_ElementBase_Init` (0x1441ffb90, 126 of 136
classes) → class init (`Vm_UiToggle_Init` 0x144272810, `Vm_UiSlider_Init` 0x1442593b0,
`Vm_SingleValueDecision_Init_Thunk` 0x144276f20). The base checks, in order, the FourCC (5), the
layout path (0x22 / 3), the first component (3) and the behaviours (3 or the behaviour init's rc); the
list base returns **3/5** without a bound `data`; `SVCD` fails without `value`; `F1TS` rejects an
`inputType` other than `normal`/`dpadOnly` with 0x57 **[L2; F1TS L1]**. What a rejection takes down is
in §8.4.

---

## 4. UI data (`.uidb`), routes (`.uir`), flows, regions {#data}

### `.uidb` — UI databases
Root object of named entries. Two shapes (`node.rs::uidb_var`/`uidb_dict`):
- **Variable**: `{ ValueType: "uistring"|"bool"|"float"|…, Name, ReadOnly: bool, Value, DBType:
  "Variable" }`.
- **Dictionary**: `{ Name, Contents: { <name>: <entry> }, DBType: "Dictionary" }`.

`ui_data.uidb` holds the `ui_menu_data` dict. The launcher adds a `swofty` sub-dict of state
vars — `last` (uistring), `online_popup_seen` (bool), and per addon-option `o<k>` (uistring) /
`s<k>` (float) (`pages.rs::state_entries`, applied by `patch.rs::ui_data`). Screen bindings then
read `#ui_menu_data.swofty.o3`, etc.

`tool_tips.uidb` holds `tool_tips.Contents.tooltips_dictionary.Contents` — per-action prompt
entries `{ Name, Contents: { action_path, text, priority } }`. `action_path` is
`#input_data.action_sets.menu_actions.actions.<action>`; the launcher clones stock entries to
add `mod_menu` / `mod_menu_offline` / `swofty_adjust` prompts (`patch.rs::add_tooltip`).

`onlineservices_messages.uimsg` holds `messages[]` = `{ id, title, messageBody, attributes[],
options[], …Tokens }` (`patch.rs::messages`).

### `.uir` — routes
Root `{ RouteGroupings: [ { Id, Routes: [...], Attributes: [{Id:"Source",Value}] } ] }`
(`patch.rs::route_group`). A **route** (`patch.rs::route`):
```
Id           stri
Commands     list of { Attributes: [{Id,Value}], Type: stri, Dependencies?: stri }
Attributes?  [ {Id:"Destination", Value:<screen>} ]
Type         si32   (0 normal, 1/2 sub-route kinds)
Dependencies stri   command-id dependency list, e.g. "2,3"
```
Command `Type`s used: `OpenScreen`, `CloseScreen`, `TransitionScreen` (Direction in/out,
Target), `InputState`, `TriggerSubRoute` (SubRoute, Target, To/From), `Migrate` (Flow,
SkipActivator). Builders: `screen_routes` (open / open_no_input / close), `subpage_route`
(close src → open dst via TriggerSubRoute), `migrate_route` (TriggerSubRoute close + Migrate +
reopen). The runner is `Vm_Run_Route` (vm 0x144188370); a missing target screen raises
`"No screen called <X>"`, which the native route guard intercepts and cancels (§8.3).

### `flows.ui` — flows / scenes / entries
Nodes: `FLOW` (a flow; prop `asset_group` = `asset_groups/…/<flow>.assetgroup`, whose basename
maps to the loading ERP, e.g. `flow_options`) → children `FLSC` (scenes) → children `FLEN`
(entries; prop `screen`). `check.rs::flows` reads this; `patch.rs::flow_entries` appends an
`enter_<screen>` `FLEN` per new screen. **A screen is only reachable if a flow that loads its
ERP lists an `enter_<screen>` entry** — else routes to it cannot open it (`check.rs::routes`).

### `region_loading.uiconfig` — regions
`regions.Contents` → `region_undefined.Contents` maps `<screen>` →
`asset_groups/ui_package/screens/<screen>.assetgroup` (`patch.rs::regions`), so the asset system
knows which assetgroup to stream for the screen.

---

## 5. The scene / `.idf` layout resource {#idf}

**SOLVED** (was the top open item). An `.idf` scene-layout resource is **itself a `!pkg`
container** (§1) — same tagged binary as every `.ui`/`.uidb`, not a `bxml` blob. The container
layer therefore already parses and **round-trips byte-identically** with `pkg.rs` / `tools/pkg.py`;
the missing piece was the *schema* inside, which is an EGO/`neon` **`eScene` scene graph**.
Decoder + write-back: **`tools/idf.py`** (reuses `pkg.py`).

In an ERP (§6) an authored `.idf` is exploded into many resources keyed by
`<path>.idf?context=<ctx>` (`render`→GfxBuffer/Mesh/Model, `animation`→AnimClip,
`animstringtable`→ACStringTable, per-element geometry, …). The **layout** lives in the single
resource of **kind `UILayout`** whose name is exactly `<path>.idf` (no `?context`); that one is the
scene `!pkg`. List them with `python3 tools/erp.py <archive> '*' | grep UILayout`.

### 5.1 Container → scene tree
```
root !pkg  →  { scenelist: [ { scene: <element>, name: <ui64>, nodeidlookup: [<ui32>, …] } ] }
```
Every stock `.idf` holds exactly **one** scene (scenelist length 1). The scene entry:
- `name`  **ui64 = the `componentId`** a `.ui` layout attach resolves to (§2.5). It equals
  **md5-64 big-endian of the `eaid://` path with the `.idf` suffix stripped**
  (`node.rs::component_id`) — e.g. `md5_64_be("eaid://ui_package/art/screens/career/rnd_dept")`.
  Confirmed on **4243/4243** stock scenes. So the `.ui` `componentId` attach (debugName
  `<path>.idf`) → this scene.
- `nodeidlookup`  the scene's **name index**: the sorted, de-duplicated list of `djb2(leaf name)`
  of every element, `"Scene Root"` included. It is **not** the elementIds. Rebuilding it this way
  reproduces the stock bytes on 933/933 decoded scenes. Layout attaches (`nodeId =
  djb2(debugName)`, §2.5) resolve against it, so an added element must add `djb2(its name)` here,
  in sorted position (`idf.py: relookup`).
- `scene`  the **root element** (name `"Scene Root"`, `typeName eScene`).

### 5.2 The element object
Each element is an object with (observed) keys:
```
name        stri   slot name a widget attaches to; root is "Scene Root"
elementId   ui32   = djb2(dotted path)  — the element's own id (a `.ui` attach uses djb2(name))
xref        ui64   eXRef only: componentId of the referenced scene (§5.5, the xref rule)
typeName    shnm   authoring class (see 5.4)
position    vec3   local translation (x, y, z[, pad])   — stored as 4 floats
scale       vec3   local scale
rotation    quat   local rotation (UI is authored in 3D; screen plane ≈ XZ, quat tips it to XY)
params      [ {value, name}, … ]   named authoring params (see 5.5), incl. minExtents/maxExtents
geoms       [ {resourceid, materialid, extentsmin(vec4), extentsmax(vec4), asset(ui64)[, isBlur]} ]
anims       [ {asset} ]   AnimClip refs (the <path>.idf/<elem>?context=animation resources)
children    [ <element>, … ]
```
**`elementId = djb2(dotted path)`**, where the path is rooted at the **top-level child's own
name** and the `"Scene Root"` element is *excluded* (e.g. `chassis`, `chassis.chassis_increase`,
`panel_stacker.card_centre.card_stacker.card_4`). Verified on **43746/43746** elements across
4243 scenes. It is **not** what a `.ui` layout attach carries: a widget's `nodeId` is
`djb2(leaf name)` and resolves through `nodeidlookup` (§5.1). So a widget binds to an element **by
name**, and a new slot must have a unique leaf name (e.g. `tile_5`).

### 5.3 Where a rect/position is stored
An element's on-screen placement is **`position` (vec3) composed down the tree** (each child is
relative to its parent), plus `scale` and `rotation`. Its *size* is the local bounding box in the
`minExtents`/`maxExtents` params (vec3, present on drawn/text elements), or the geom
`extentsmin`/`extentsmax` (vec4) when no extents param. `tools/idf.py tree|slots|find` prints
`pos`, `size(w,d,h)` (w=x, d=z, h=y), `scale` per element.

### 5.4 Authoring classes (`typeName`)
```
eScene    scene root                 eNull     transform/group (most common)
eXRef     external component ref (element key xref = the referenced scene's componentId;
          params fileRef = that .idf, XrefMaterialBinding = its .emb)
eText     text/label (params style, halign, wrapping, textcase, boxWidth, maskWidth/Height, document)
ePoint    anchor point               eAnchor   anchor
eStacker  linear/grid layout container (params stackerType, gap, useStaticGap)
eCentre   centring container         eScaler   scaling container (params scaleX/Y/Z)
eWindow   window/clip                eBoundless  unclipped group
eMaskTexture / eMask  texture mask   eScroller scroll region   eCanvas  canvas   eCamera camera
```
### 5.5 Param vocabulary (name → meaning)
`className` (the class, mirrors typeName) · `guid` (per-element uuid) ·
`pivotRot`/`pivotTrans`/`scaleRot` (transform pivots) · `minExtents`/`maxExtents` (local bbox) ·
`eLayer.layer`/`eLayer.layerIsOffset` (z-order) · `fileRef`/`XrefMaterialBinding` (eXRef targets,
see the xref rule below) ·
`eAnimationGroup.AnimGroup` · eText: `style boxWidth maskWidth maskHeight halign wrapping textcase
document preview*` · eStacker: `gap stackerType('l'=linear) useStaticGap` · eScaler:
`scaleX/Y/Z eScalee.enabled` · `width height` · `eRTL.*` (locale direction) · `eMaskTexture.*`.

**The eXRef xref rule.** An `eXRef` element carries the element key `xref` (ui64) = md5-64
big-endian of the referenced scene's `eaid://` path without `.idf`, i.e. that scene's `componentId`,
besides its `fileRef` param. The game loads the scene whose componentId is `xref`, whatever `fileRef`
says; every stock `eXRef` has `xref == md5-64(fileRef)` (58,925 references). Changing only `fileRef`
keeps the old scene in game while a `fileRef`-following viewer shows the new one: the R&D pie's
`chart_1` pointed at `rnd_dev_potential5.idf` with the stock `xref` drew the stock 4 wedges **[game]**
(`rnd_ui.md#pie`). A scene edit sets `xref` and `XrefMaterialBinding` with `fileRef` (`idf.py retarget`);
`check.rs` reports `xref-mismatch` as a hard finding.

### 5.6 Two layout patterns (the fixed-vs-dynamic distinction that governs the R&D box)
- **Fixed grid** — boxes are distinct sibling elements each with an explicit `position`.
  e.g. `art/screens/career/rnd_overview.idf` (research-overview page) places 4 department tiles
  `tile_1..tile_4` (each an `eXRef`) via `transition_null_1..4` parents on a 2×2 grid
  (cols x≈2.07/6.42, rows y≈-1.16/-3.68) plus 2 info panels `secondary_panel_*`.
- **Dynamic stacker** — boxes are `eXRef` children of an `eStacker`, which re-lays-out *all* its
  children at runtime by `gap`. e.g. `art/components/career/laptop/overview_panel.idf` has
  `card_stacker` [eStacker, stackerType l, gap 0.2, useStaticGap false] with **5** pre-authored
  slots `card_0..card_4` (eXRef → `overview_card.idf`); vertical pill stacks
  (`rnd_perf_stack.idf`, `r_and_d_performance_compare_stack.idf`) use the same shape with
  `team_0`/`pill_1..N` slots. A stacker lays out however many children are attached.

The `.idf` is **no longer the blocker** for a 5th on-screen rect: a fixed-grid scene gets a 5th
box by adding a sibling element (5.7); a dynamic stacker already sizes to N children.

**Which container each R&D surface uses (resolved from the assets):**
- **R&D > Performance department tiles** = `screens/laptop_r_and_d.ui` (in `flow_career_hub.erp`
  and `flow_team_hq.erp`), template `tab_content_rnd_upgrades_alt`, which instantiates
  `rnd_overview.idf` (slot 5) and holds 4 authored `TILE` nodes `aero/chassis/engine/maintenance`
  attached by name to `tile_1/tile_2/tile_3/tile_4` with `dataContext=@aero|@chassis|@engine|
  @maintenance`. The same template carries the `init_pie_sector` dev-potential pie. This is the
  **fixed 2×2 grid**: `tile_N` eXRefs (→ `tiles_rnd/3x4_rnd_overview.idf`, 4.15×2.32) under
  `transition_null_1..4` at x 2.075/6.425, y −1.16/−3.68. A 5th box needs a `tile_5` element,
  a 5th `TILE` node attached to it, and a 5th per-department VM child for its `dataContext`.
- **`Career_Build_RnDDeptBoxGrid` (0x141c85f70)** fills a different screen,
  `laptop_r_and_d_departments_container` (`flow_r_and_d.erp`); its `Vm_Set_*` leaves
  `show_shoulders`/`block_input`/`career_hub` are bound only there. Its cards are **tabs** of
  `tab_controller [UITC]`; each header is a `laptop_r_and_d_tab_header [UITH]` instancing
  `primary_icon_nav_item`, stacked into `tabContainer = …nav_left.Scene Root.primary_nav_stacker.
  bg_scaler.primary_toggle_stacker`, an **`eStacker` with no authored children**
  (`components/smart/primary_toggle_navigation.idf`). The injected `card[6]` is the 7th tab,
  appended after the purchase and perks panels. It never binds into `rnd_overview.idf`; there is
  no index or elementId binding between the two.
- `tab_content_research_overview` (same `.ui`) uses `team_hq/research/overview.idf`, a third
  4-tile scene.

**The R&D "development potential" pie is also an authored `.idf` scene — but its wedge *shape* is
baked mesh geometry, not a scene parameter.** `art/components/career/rnd_tree/rnd_dev_potential.idf`
holds the pie as 4 `eXRef` sector nodes `sector_1..sector_4` under `null_1`, each referencing the
shared wedge component `rnd_dev_potential_sector.idf` and **rotated 0/90/180/270°** (the `rotation`
quat; `(0,0,0,-1) (0,0,.707,-.707) (0,0,1,0) (0,0,-.707,-.707)`) so four 90° arcs tile the circle.
The wedge arc itself is a **GfxMeshRes** (`rnd_dev_potential_sector.idf/comp_potential?context=
render&lod=0&mesh` + its `vb=/ib=` buffers), referenced by the layout node's geom `resourceid`;
the scene stores the element's position/scale/rotation/extents, **never the arc angle**. So adding
a 5th slice splits into: (a) a scene edit `idf.py` can do — add a `sector_5` `eXRef` and re-set the
five `rotation`s to 72° steps (0/72/144/216/288°); plus (b) a **separate art task the `.idf` format
does not cover** — a new 72° wedge *mesh* (reusing the stock 90° mesh at 72° spacing overlaps by
18°/wedge); plus (c) the native `Career_Populate_RnDPieSlices` guard emitting the 5th slice (career
lane). Contrast the box card, which is a generic reusable `eXRef` template needing no new geometry.
This makes the pie's authored-node blocker **the same `.idf` lever for the node, but with an extra
baked-mesh dependency** the box does not have.

### 5.7 Editing / adding a slot (write-back)
`pkg.py`/`idf.py` re-serialise byte-identically, so the scene is editable. To add a box/slot clone
an existing sibling and fix its identity:
1. deep-copy the sibling element (`idf.py: clone_node`), set a new **unique** `name` and a fresh
   `guid`;
2. set its `position` (for a fixed grid, copy the sibling's y and bump x by the column step; for a
   stacker, position is recomputed at runtime so any value works);
3. **recompute `elementId = djb2(dotted path)` for the clone and every descendant** (their paths
   changed) — `idf.py: recompute_ids`;
4. append the clone to the parent's `children` list and rebuild the scene's `nodeidlookup` as the
   sorted unique `djb2(leaf name)` of every element (`idf.py: relookup`).
`tools/idf.py clone-slot <file> <src> <new> <x> <y> <z> <out>` does exactly this; the result
re-parses with every `elementId` consistent and round-trips. (`set-pos` moves one slot.)
Validation: all 4243 stock scenes round-trip identical. (Before 2026-10-19 `clone-slot` appended
the clone's elementIds unsorted to `nodeidlookup`, which drops its name from the index.)

**Animated parents.** An element with `anims` (e.g. `rnd_overview.idf`'s `transition_null_N`)
plays an AnimClip whose channel is the element's name and whose keys are **absolute** local
transforms: translation from the stock position (±0.5 y), rotation identity, scale 1. Moving or
scaling such an element is undone whenever the transition plays. Re-lay its **unanimated
children** instead (set `tile_N.position` as an offset from the parent and `tile_N.scale`), and give
a cloned animated parent the **same position** as its source so the shared clip resolves the same
either way.

**Shipping the edit (built, launcher PR #138).** The scene is the single fragment of the
`UILayout` resource `<path>.idf`; the addon replaces it with `operations.erp` `replace-resource`:
```json
{ "op": "replace-resource",
  "erp": "2025_asset_groups/ui_package/flow_career_hub.erp",
  "resource": "eaid://ui_package/art/screens/career/rnd_overview.idf",
  "source": "ui/rnd_overview.idf" }
```
One stanza per archive that carries the scene (`rnd_overview.idf`: `flow_career_hub.erp` and
`flow_team_hq.erp`; shared components can sit in 6+). The build checks that a `UILayout` source is
a scene whose `name` equals `component_id(<path>)`. `Resource::set_fragment` recompresses it,
updates `size`/`packed`/md5, and `ErpFile::write` re-lays every offset and the index, streaming the
other resources byte for byte. The runtime overlay refuses a resource whose fragment sizes differ
from stock, so any archive with a resized replacement is **rewritten into the game files**
(journaled; `swofty restore` puts it back), and the build prints `erp: <archive> built into the game
files; …`. A same-size replacement still goes to the overlay. It warns when another stock archive
in the package folder also carries the resized resource, since the first archive to load it wins.
Validated: an unmodified `rnd_overview.idf` round-trips with every resource equal to stock; a grown
one rewrites both archives with all 10343/19723 other resources byte-identical and restore leaves
the folder byte-identical.

**Still open:** the semantics of a few rarely-used params (`document`, `preview*`, `eRTL.*`) are
name-only guesses.

---

## 6. ERP UI archives {#erp}

The resources above ship inside `.erp` archives (ERPK). Format in
`packages/formats/src/erp.rs`; user docs `docs/reference/erp.md`, `docs/reference/ops/erp.md`.

### Layout
```
header[0x30]   "ERPK", version @+4, data_off @+0x20 (u64), info sizes @+0x10/+0x18
u32 resource_count   u32 total_fragment_count
per resource:
  u32 entry_len   u16 name_len   name (NUL-padded)   kind[16] (type string)
  unk[6 if v>=4 else 4]   u8 fragment_count
  per fragment: name[4], offset u64, size u64 (uncompressed), flags u32,
                compression u8, packed u64
  md5 hash[16]   (content hash over decompressed fragments)
<data blob at data_off>  concatenated fragment bytes
```
`compression`: `ZSTD=0x11`, `STORED=0x91` (zstd level 19). `Fragment::data()` decompresses;
`Resource::content_hash()` is md5 over decompressed fragments. Resource `kind` strings: `"UI"`,
`"GfxSurfaceRes"` (textures), `"…Res"`, etc. Resource **names are `eaid://` URIs** resolved by
the `assets` lane, e.g. `eaid://ui_package/screens/fe_home.ui`.

### Which UI ERPs hold what (`2025_asset_groups/ui_package/`, `ui/mod.rs`)
| ERP | Resources the launcher touches |
|---|---|
| `persistent.erp` | `routes/routes_frontend_general.uir`, `flows.ui`, `region_loading.uiconfig` |
| `databases.erp` | `databases/tool_tips.uidb`, `databases/ui_data.uidb` |
| `flow_persistent.erp` | `message_data/onlineservices_messages.uimsg`, `screens/persistent.ui` |
| `flow_frontend.erp` | `screens/fe_home.ui`, `screens/fe_career.ui`, other frontend `.ui`s |
| `flow_options.erp` | `screens/settings_container.ui` — **read-only source** (templates + chrome) |

A resource can have several fragments; fragment index matters. The launcher reads
`settings_container.ui` **fragment 1** for the `Templates` array and **fragment 0** for the
chrome (`mod.rs::build`, `read_frag`), and reads `fe_home.ui` from its copy in
`flow_frontend.erp`. List real names with `python3 tools/erp.py <archive> '*'`.

### How the launcher patches an ERP (`mod.rs::write_erp`)
It opens the source ERP, replaces fragment 0 of named resources with transformed bytes
(`Entry::Owned`), copies every other resource byte-for-byte by streaming its original fragment
span (`Entry::Copy`), and writes a fresh index. Round-trip identity holds for untouched
resources. Addon `operations.erp` uses the same writer (`erpbuild.rs`); since PR #138 an archive
whose replacement changes a stock entry's size is written this way even on the runtime route
(§5.7). The patched archives land in the overlay game-link; new `.ui` **screens are written
as loose files** under `swofty/ui/` and registered at runtime (§7/§8), not inserted into ERPs.

---

## 7. The runtime view-model binding {#runtime}

How an authored node tree becomes a live tree with real data.

### Node class → live node
On screen load the UIDF handler (`Ui_Uidf_Handler`, native target in `ui.cpp`) drives
`Vm_Load_UidfScreens` jobs (mgr+0xb900). Per node it looks up the class factory (§3), reads the
node from the `.ui` data, then runs the class's from-data init. Templates (§2.9) go through the same
reader and inits each time they are instantiated (`Vm_Build_TemplateNode`, `vm.md#templates`). The engine resolves each
binding's `dbentry` through `Vm_Resolve_RegistryNode` (0x1442b1b60) into a node in
`g_Vm_Registry` (0x146ca54f8), so a bound property mirrors a registry value. List controls
spawn one child per element of their bound `data` array (§2.3).

### Publishing values from native (the core VM API)
`vm.md`: `Vm_Set_String(reg, parent, name, NeStr**, overwrite)` /`Vm_Set_Bool`/`Vm_Set_Int`
(0x140d38fd0)/`Vm_Set_UInt`. `parent` is a registry node + 8; the leaf resolves via
`Vm_Resolve_NodePath_Thunk` (0x1442b2770). `overwrite=1` replaces. Names/pins are djb2; the
registry node tree is the live data a binding reads.

### The launcher's native view model (`native/src/core/frontend/ui_vm.cpp`)
When a screen sets `allowVMBinding` and is declared `view_model:true`, `ui_vm_attach`:
1. allocates a 0x300 native VM object;
2. synthesizes its vtable by **copying `FrontendTransitionViewModel`'s vtable**
   (base+0x57391a0) + a helper vtable (base+0x5a72fe0), then overriding slots: `2` destroy,
   `5` on_message, `6` on_bound, `10` on_command, `12` on_update, `13` on_activated,
   `15` on_deactivated;
3. registers it with `Vm_Register_ViewModel` (0x42bf450, mode 2);
4. sets `path_ = "view_model_data." + screen`.
Then on `on_activated` the VM publishes data via `set_text/set_bool/set_int`
(→ `Vm_Set_*`). The shipped `AddonPage` factory (prefix `swofty_addon_`) publishes `title` and,
per option `k`, `o<k>.{label,text,value,index,min,max,step,is_slider}`, and handles the commands
`swofty_next:/prev:/set:<addon>:<option>[:<index>]` (`on_command` → `settings_command`). The
authored rows bind the matching leaves (`pages.rs::vm_page`: slider `value=@o<k>.value`,
`min/max=@o<k>.min/max`, label/value text `@o<k>.label`/`@o<k>.text`).

### Native UI builders (stock, cross-ref to career R&D)
Some screens are not data-spawned at all — native builders construct the live nodes directly.
The R&D Performance page (`career.md`) is the canonical example and the reason the **runtime
route** exists:
- **Box grid** — `Career_Build_RnDDeptBoxGrid` (0x141c85f70): 4 literal
  `Career_Construct_RnDDeptCard` (0x141c7aac0) calls; card vector sized to exactly 6 ptrs. A card
  renders through its own card VM node (`card+0x268`); the icon is an **atlas path**
  `<pkg>.rnd_dept_icons_small.<frame>`, not a VM node, so a card needs **no** authored UIDF node.
  v0.3.0 added a live 5th card by native inject at `site_Career_RnDDeptBoxGridInject`
  (0x141c86567) growing the vector 6→7 and letting the builder's attach loop bind it; the builder
  *populates VM node `view_model_data.laptop_r_and_d_departments_container`*. Decoded (§5.6): the
  cards are **tabs** of that screen's `tab_controller [UITC]` (`builder+0x2d8`); the headers stack
  in `primary_toggle_stacker`, an `eStacker` with no authored children, so the 7th (Tyres) tab
  needs no scene edit. It is **not** the R&D > Performance tile grid, which is the fixed
  `rnd_overview.idf` grid with 4 authored `TILE` nodes in `laptop_r_and_d.ui`; the inject cannot
  put a box there. A 5th Performance tile is `tile_5` (scene) + a `TILE` attached by name + a VM
  child for its `dataContext` (§5.6–5.7).
- **Pie** — live builder `Career_Build_RnDPieOverview` (0x141f46eb0) + `Career_Populate_RnDPieSlices`
  (0x141f48030) on class-B vtable 0x14575e300; the research-overview pie `Career_Build_RnDPotentialPie`
  (0x141f41d60, class A) allocates a per-sector buffer of **exactly 0x900** and guards
  `code<4`/`i<=4`. Sectors are pre-authored VM-node handles; a 5th wedge needs a 5th authored
  sector node **and** a relaxed native guard (blind relax overruns the buffer → crash).
These are **hardcoded-4, not data-driven, and not reachable through the authored UIDF/ERP
route** — they are the textbook "you must use a native mixin" case (§8.2). The R&D-specific fix
specs are owned by the `career` lane; this reference only names the surfaces.

---

## 8. How to EDIT / ADD UI — the recipes {#recipes}

Pick the route by *what* you are changing. Rule of thumb: **data/labels/routes/new whole
screens → authored-asset or runtime-screen route; changing how the engine builds a live tree
(counts, geometry of a native-built widget) → native mixin.**

### 8.1 Change an existing screen's text / props / signals (authored-asset)
Target a stock resource and edit its nodes. The planned declarative form is a
`*.pkgpatch.json` listed in `operations.ui` (`docs/reference/ops/ui-patches.md`): ops
`set-text`, `set-props`, `clone-node`, `add-signal`, `remove-signal` against a `root` node path;
for `.uir`/`.uidb`, `clone-entry` / `set-value` with dotted paths and `[field=value]` selectors.
**Status: `operations.ui` is NOT built** — `build.rs` skips it with
`"<id>: N UI operation file(s) skipped; that stage is not built yet"`. Today the same
`clone-entry`/`set-value` edits run only through the `operations.erp` **`patch-pkg`** op
(`erpbuild.rs`/`pkgedit.rs`), and even there **only `clone-entry` and `set-value` are built** —
any other `patch-pkg` edit prints `"… is not built yet"` and the archive is skipped. The whole-resource
ops (`replace-resource`, `put-resource`, `add-/copy-/remove-resource`) are built (launcher #82), so a
screen or scene edited offline (`pkg.py`/`idf.py`/`uinode.py`) ships as a `replace-resource` of
the `.ui` (`UIDF`) or `.idf` (`UILayout`) resource (§5.7). Otherwise programmatic screen edits go
through `patch-pkg` (uidb/uir/layer entries) or are done in Rust like the launcher does
(`node.rs` helpers + `write_erp`). The launcher's own menu build is the worked example:
`clone_node`, `instance` (clone a `Templates` entry, rename refs, strip bound data, drop
`command_vmContextValid` events), `literal_label`, `add_events`, then `fix_ids`.

### 8.2 Add a new widget/box/tile to an existing native-built screen (runtime mixin)
When the screen is built by native code (R&D boxes/pie, the home tile row), author nodes won't
help — hook the builder. Pattern (`docs/reference/addons/in-game-ui.md`, `fe_home.md`):
- **Home tile**: a site mixin at `site_FeHome_AfterCareerCall` calls `FeHome_Add_CareerTile`,
  `swofty::subclass`es the new item's vtable (slot 4 `populate`, slot 5 `get_action`), sets its
  `id`, bumps the builder's counters; `populate` fills the tile's VM node via `Vm_Set_String`,
  `get_action` returns `{type:1, str:<route>}`.
- **R&D box/pie**: grow the builder's card/sector vector in a mixin and build a real card/sector
  into the new slot (career lane). Remember §5: the layout rect may still be missing.

### 8.3 Add a whole new screen (runtime screen registration — built, works)
`operations.screens` in `swofty.json` (`docs/reference/addons/in-game-ui.md`):
```json
"operations": { "screens": [
  { "name": "my_page", "file": "ui/my_page.ui", "flow": "flow_options",
    "anchor": "settings_container", "view_model": false }
] }
```
`swofty build` installs the `.ui` as `swofty/ui/<name>.ui` and lists it in the `ui` block of
`swofty/settings.json` (`build.rs`, `settings.rs::UI_DIR`). At runtime `native/src/core/frontend/ui.cpp`:
- hooks the UIDF handler; when the screen's **anchor** resource loads (its name must appear in
  the anchor's `.ui` bytes), it arms the screen and, at the next screen-open, registers it
  through the engine's own screen loader (`Ui_Uidf_LoadScreens`) — so the game builds it exactly
  like a stock screen;
- installs a **route guard** on `Vm_RouteCmd_ResolveTargetScreen`/`Vm_RouteCmd_OpenScreen`:
  a route to an unregistered *addon* screen ("No screen called X") is cancelled and the last
  screen reopened, instead of crashing the front end;
- mid-hooks the node read/init/factory/screen-result sites and logs every rejected node with the
  reason to `swofty/loader.log` (invaluable when a screen won't open).
Set `view_model:true` to get a native VM (§7); swofty_core attaches one if a factory prefix
matches the name. To be *reachable*, the screen also needs a route (`.uir`), a flow
`enter_<screen>` entry (`flows.ui`), and a `region_undefined` entry (`region_loading`) — which is
exactly the set of stock-ERP patches the launcher applies for ADDON SETTINGS (`mod.rs::build`).

### 8.4 What a rejection takes down, and what `check.rs` enforces
A node the engine rejects takes down more than itself, and nothing logs it (`vm.md#failures`) **[L2]**:
- in a **screen**: that screen and every later screen of the same `.ui` (a same-name screen, 0x55,
  only skips itself);
- in a **template instance**: the whole instance (a failing root attaches nothing; a failing
  descendant leaves the partial tree attached) **[L2, run]**;
- in a **tab content** template: that tab's content instance, dropped silently; the controller keeps
  its headers **[L2, run]**; how it then maps tabs to the remaining contents is not mapped, and with
  the era picker it emptied the F1 World play screen **[run; mapping inferred]**;
- a **duplicate template name**: every later template of that resource **[L2]**.

`packages/core/src/games/f1_25/ui/check.rs` checks every edited `.ui`/`.idf` against its stock copy before a build
ships (addon `operations.erp` results, `operations.screens` files, the core menu build) in three layers
(`docs/reference/ui/patterns.md#checking-ui-edits`):
- **hard, engine** — the rules in `re/db/ui/loader_rules.json`: `node.record`,
  `node.class_registered`, `layout.path_nonempty`, `layout.step_keys`, `components.keys`,
  `behaviours.type`, `commands.entry`, `events.entry`, `signals.entry`, `listbase.data_binding`
  (bind `data=@<array>`; for `UITG` also `selection=@<value>`), `svcd.value`, `f1ts.input_type`,
  `behaviour.nbvn_visualnav`, `template.duplicate`, `screen.duplicate`; each finding cites the rule
  id, level, code, scope and function address;
- **hard, format** — stale node/event/command/signal hashes, signals to nodes missing from their root,
  `xref-mismatch`, `scene-id`, `nodeidlookup`, missing scenes and layout steps the edit no longer
  resolves;
- **warnings** — anything the stock catalogue never does (types, props, shapes, pins, links, layout
  chains), view-model keys no stock node of the template binds (`vm-key-new`) and engine-read props the
  edit drops (`prop-dropped`).

By hand: `cargo run --release -p swofty-core --example ui_check -- <stock.erp>=<built.erp>`. A failure the
checks cannot see (ordering, bindings, attach-time resolution) is traced live with
`re/tools/ui_trace/ui_template_trace.sh` (`vm.md#trace`). The older launcher-side checks still apply:
- a non-root node sets `allowVMBinding`;
- `allowVMBinding` is set without an attached view model, or vice-versa;
- a route's target screen is not listed by any flow that loads the right ERP; a `TriggerSubRoute`
  names a nonexistent sub-route; a `Migrate` names an unknown flow.
When patching **stock** nodes, `check.rs::stock_kept` additionally refuses any change to a stock
node's props/layout/behaviours/stock-children, or any signal added to a stock node that no stock
node already sends — the invariant that keeps the launcher's popup gate from altering the game's
own popups. Mirror this discipline in addon patches. Always `fix_ids` after editing.

---

## 9. Hashing & tooling {#tooling}

Hashes used across the formats:
- **djb2** (`h=5381; h=h*33+byte`) — node names/`id`(full path)/`nameId`(name), event/command
  names, signal `to_object`/`to_pin`, layout `nodeId`(debugName). (`node.rs`, `ui.cpp::djb2`.)
  In a scene `.idf` the same djb2 produces each element's **`elementId` from its dotted path**
  (top-level child rooted at its own name, `"Scene Root"` excluded; §5.2) and the `nodeidlookup`
  name index from each **leaf name** (§5.1). A `.ui` `nodeId` attach is `djb2(leaf name)`.
- **fnv1a32** — `!pkg` v1 object key hash + sort order (`pkg.rs`).
- **md5-64** (first 8 bytes of `md5(path)`, big-endian) — layout `componentId` → `.idf`
  (`node.rs::component_id`). The hashed string is the `eaid://` path **with `.idf` stripped**;
  the target scene's `name` ui64 carries the same value (§5.1, confirmed on 4243 scenes).
- **ci-fnv1a-64** — the career department enums (career lane, not a UI format).

Tools (`/mnt/work/VSC/F1ModManager/tools/`, mirrored in the `modlist` worktree `tools/`):
- `erp.py <archive> '*'` — list resource names; extract resources.
- `pkg.py` — read/write `!pkg`; `idf.py` — decode/edit scene `.idf` layouts (`tree|params|geoms|
  slots|find|scene-id|roundtrip|clone-slot|set-pos`, §5); `uinode.py` — nodes + IDs;
  `uitree.py <file> [--full]` — print the node tree (events/signals with `--full`); `uiops.py` —
  build/patch UI node ops; `ui_catalog.py` — catalog; `erpw.py`/`erpx.py`/`erpdump.py` —
  rebuild/extract/dump archives.
- Rust: `packages/formats/src/{pkg,erp}.rs`; `packages/core/src/games/f1_25/ui/{node,pages,patch,check}.rs`
  (generator), `packages/core/src/{erpbuild,pkgedit}.rs` (`operations.erp`/`patch-pkg`).

---

## 10. Open / unknown {#open}

- **`.idf` scene-layout binary format** — **SOLVED** (§5): it is a `!pkg` `eScene` graph, decoded
  + round-tripped + write-back by `tools/idf.py`; componentId/elementId derivations confirmed on
  4243 scenes / 43746 elements. An edited `.idf` ships with `operations.erp` `replace-resource`
  (§5.7, launcher PR #138). Remaining: a few rarely-used element params (`document`, `preview*`,
  `eRTL.*`) are name-only guesses.
- **Node slot 7** — empty in authored screens; role unread (slot 5 is the components list, §2.1).
- **Attach-time layout resolution** — how a stored layout step resolves through `eXRef`/`eMask`
  elements when the element attaches (0x144218810) is not mapped.
- **Tab content mapping** — how `UITC` maps a selected tab to its content instance when one is
  missing.
- **Node-class roles** — only ~20 of ~97 FourCCs annotated (`vm.md#classes`); the rest are L1.
- **`operations.ui` pipeline** — not built (`build.rs`); `set-text`/`clone-node`/`set-props`/
  `add-signal`/`remove-signal` are a planned format. `operations.erp` `patch-pkg` builds only
  `clone-entry`/`set-value`.
- **Language (`lng_*`) authoring** — not built; new text must be literal (`localise=false`).
  New `lng_*` keys for new strings can't be added through an addon yet (noted in `career.md`
  as a blocker for the `lng_rnd_dept_tyres` name).
- **Runtime confirmation** — the template path (instantiation, tabs, NBVN) was traced live on
  2026-10-23 (`vm.md#trace`), but the logs are not filed as a T### run, so those rows are L2; the
  binding/route path is L2 (static); `vm.md`/`fe_home.md` flag `Vm_Fire_Route` + label rendering as
  needing a trace test.

## Inbox
