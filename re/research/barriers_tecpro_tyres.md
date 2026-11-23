# TecPro and tyre walls: prop systems, joints, ropes (static + asset data)

vmknolle, 2026-11-23. Builds: Steam d873a1c8 (the KB build, decomp read 2026-11-06) and Steam 26566308 (signature
check only). Everything here is STATIC or read from the stock asset files; nothing is runtime-confirmed. Question
from the vmknolle barrier-impact add-on: why do TecPro and tyre walls barely move or deform when a car hits them,
and what can a hook change without new assets?

## Verdict

- **The deformation is already in the data.** A loose TecPro set is 2-6 rigid blocks on joints, a loose tyre wall
  section is 6 bodies on 10 joints; both have a skinned render mesh, so the line bends and the wall dents
  ([assets](#assets)). A single TecPro block is one rigid body: it cannot squash.
- **What keeps them in place is code, not data.** `Game_Breakable_Activate` ties every loose piece to the world
  point where it stood with a one-sided distance joint ("rope", 0.3 m TecPro, 1.0 m tyres) and joins neighbouring
  TecPro blocks with joints whose numbers are written in code, not taken from the templates ([joints](#joints)).
- **Intact barriers have no body.** Only the pooled loose copy can move; the intact prop is anchored. A barrier
  only deforms after activation.

## Assets

Read from `PYRB` resources `...?context=physics_objects` (lexical format: u32-length strings + u32/f32 words) in
`tracks/<slot>/wep/<slot>_common.erp`. Values are the words after each surface template name; read as half extents
of a box (HYPOTHESIS for the field meaning, the sizes match the real barriers).

| prop system | where | static part | dynamic part | joints |
|---|---|---|---|---|
| `tecpro_dynamic_main_a` | abu_dhabi | 1 box, half 0.736 / 0.6 / 0.3 | 1 body `dynamic_collision_bone_a`, skin `tecpro_dynamic_main_a_skin`, half 0.74 / 0.6 / 0.308 | none |
| `tecpro_dynamic_main_a_m02` .. `_m06` | mexico (m02), abu_dhabi (m03 / m04 / m06) | 1 box over the whole set (m03: half 2.217 / 0.6 / 0.3) | 2-6 bodies `dynamic_collision_bone_a/c/e/..`, one skin `..._Mxx_skin` | n-1 x `cDeformableRubberLateral` between neighbouring bones (m03: a-c, c-e) |
| `core_barr_physical_tyres_a_main_01` | austria | 1 box half 2.0 / 0.55 / 0.5 | 6 bodies `dynamic_collision_lf/cf/rf/lb/cb/rb` (front / back row x left / centre / right), half 0.675 / 0.55 / 0.255, one skin | 10: 4 x `cDeformableTyreWallLateral` (along a row), 4 x `cDeformableTyreWallEnds`, 2 x `cDeformableTyreWallCentre` (front-back) |
| `physical_tyres_base_a` | brazil | same template set as above | | |

- Surface templates: `sDeformableRubber` (TecPro), `sDeformableTyreWall` (tyres). Body template `rbDefault`.
- A TecPro block of the model is 1.48 x 1.2 x 0.62 m; the real TecPro module is 1.5 x 1.2 x 0.6 m, about 120 kg.
- Contact materials seen by the car: `DRUB` (TecPro), `SPLA` (plastic; the Bahrain TecPro), `TYRW` / `DTYR` / `TYR*`
  (tyre walls). `TYRW` in the static track mesh has no breakable component: it can never be woken.

## Joints

Static, read in the decomp of `Game_Breakable_ActivateCore` (Steam d873a1c8). Constraint descriptor 0x1b0 bytes,
passed to `Ne_PhysWorld_CreateConstrainedBody` (0x1440cd9b0).

| item | finding |
|---|---|
| rope | every loose piece: descriptor +0x1a0 = 1 (distance mode), rest length +0x188 = 0.3 m (tecpro) / 1.0 m (physical_tyres), anchor = the world point the piece stood at. Row builder 0x1440f6e20 (4 rows): one-sided, bias max(0, d - rest), impulse 0..inf. The break test "max(10, 5 x rest)" is skipped for world anchors (constraint +0x30 set). A piece can never get further than the rope. |
| chain | neighbouring TecPro blocks (comp +0x258 group): descriptor +0x1a0 = 0, stock numbers of `cDeformableRubberLateral` written in code: linear limits +-0.1 / 0.05 / 0.05 m at +0x20 / +0x30, rotation limit +0x144, K +0x14c, twist limit +0x160, K +0x168. A patch of the template does not reach these joints. |
| call sites | three calls from Activate into the constraint ctor; return addresses relative to the breakables contact callback (`Game_Breakable_OnContact` 0x14458bb90): +0x74a chain, +0x2823 and +0x2ab4 ropes. |
| ctor | reads the descriptor only at entry plus one re-read of +0x198; max offset 0x1a8 < 0x1b0; loads it with `vmovaps` from [rsi+0x80..0x130], so a copy passed instead must be 16-byte aligned. |
| templates | joint templates `cDeformable*` live in the physics library the PYRB loader (0x144454260, `Game_PhysicsObjects_LoadResource`) fills. Float offsets used by the add-on (L1): linear limits +0x30 / +0x40 / +0x50 / +0x60 (3 floats each), spring K +0x70..+0x78, rotation limit +0x8c K +0x94 S +0x98, twist limit +0xa4 K +0xac S +0xb0, soft impulse +0xc4. |
| safe point | `Ne_PhysicsWorld` step returns in the middle of the last substep (world +0xcf0 = 1). `Activate` and `Ne_PhysBody_ApplyImpulse` are safe after `Ne_PhysUpdateStage_FinishStepNotifyBodies` (0x1440c7d70), state 0, serial: the place the game flushes its contact callbacks. `ApplyImpulse` needs 16-byte aligned impulse and point (unaligned crashed in game, 2026-11-05). |
| activation | `Game_Breakable_ParseActivation`: `tecpro` / `physical_tyres` in the object name -> activation impulse 200. Loose copies are pooled (10, TecPro 60); a full pool evicts the copy farthest away. |

## Steam 26566308

The add-on's nine byte signatures (bi_bindings.hpp) each match exactly once in `.tls$` (= `.xtls`) of the Steam
26566308 exe; the three ctor call sites keep the same offsets from the contact callback.

| function | d873a1c8 | 26566308 |
|---|---|---|
| Ne_PhysMaterial_CombinePair | 0x1440e7350 | 0x1440e9920 |
| Car_Contact_ModifyVehicle | 0x142f04040 | 0x142f065d0 |
| Game_Breakable_ParseActivation | 0x144588c90 | 0x14458b500 |
| Ne_PhysUpdateStage_FinishStepNotifyBodies | 0x1440c7d70 | 0x1440ca340 |
| Ne_PhysWorld_CreateConstrainedBody | 0x1440cd9b0 | 0x1440cff80 |
| Game_PhysicsObjects_LoadResource | 0x144454260 | 0x144456830 |
| Ne_PhysBody_ApplyImpulse | 0x1440c12e0 | 0x1440c38b0 |
| Game_Breakable_OnContact (callback) | 0x14458bb90 | 0x14458e400 |
| ctor call returns (ropes) | 0x14458e3b3 / 0x14458e644 | 0x144590c23 / 0x144590eb4 |
| ctor call return (chain) | 0x14458c2da | 0x14458eb4a |

## Open (needs a game test)

- The meaning of the PYRB words after the surface template (half extents) and of the template offsets.
- Whether wider tyre wall joints (`cDeformableTyreWall*`) make the skinned wall dent visibly, and how far a loose
  TecPro block may go before it ends up on the racing line.
