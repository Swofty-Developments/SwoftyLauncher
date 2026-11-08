# EA fuel binding candidate — research only

The player has requested native fuel support for the EA App executable, while
keeping the launcher's existing automatic Steam/EA build selection. This note
records findings; it does not authorize game writes or mark the layout as
verified. The core's existing non-Steam guard remains in place.

The inspected EA image is the build already identified in `re/db/builds/ea.json`
(SHA-256 `cc506879c1c834aa54d3478b2f2133d2ec243f2f2d313397631145ea618ff7f4`,
PE timestamp 1789128892). The running image's controller input routine was found
from the existing `site_Sess_ControllerSystem_ApplyRecordCall` signature. Its
EA call target is RVA `0x3887480`. Near RVA `0x3888a0c`, the instruction bytes
`48 8b 3d 55 bf 91 02` load a pointer from RVA `0x61a4968` into RDI.
At RVA `0x3888c8f` the routine reads the car's ECU at `+0x548`, compares the
mode at `+0x128`, then at `0x3888cab` checks the dword at
`[RDI + vehicle * 0x20 + mode * 4 + 0x28]` for zero before switching mode.

In a read-only live observation with 21 loaded cars, the global held a non-null
pointer. Its first three cars had entries `(256, 0, 256, 256)`, consistent with
Standard offered while the other modes are locked. All 21 ECU owner pointers
matched their vehicle table entries. For cars 0–19, the decoded stock fuel
tables showed lean burn 0.7, lean power 0.9 and the other entries 1.0; the
twenty-first car's entries were all 1.0. Mode values were 1 for the first 20
and 2 for the last. These are read-only observations, not proof of write,
restoration, event timing, object lifetime, or a working race MFD. Addresses
are research observations for this build, not stable offsets for other builds.

Candidate code pattern at the load:
`48 8B 3D ?? ?? ?? ?? 49 8B 9C 24 A8 05 00 00 48 85 DB 0F 84`.
The candidate matched exactly once in the running EA `.rdata` image, at the
load described above. Before adding it to the binding knowledge base, identify
an equivalent Steam pattern and use the normal KB and bindgen workflow. Record independent verification and the
required in-game trace under `re/PROTOCOL.md`. Do not reuse the EA RVA on Steam
or force `runtime::same_build()` to return true for EA.

The source change adds a fuel-only capability boundary and an **experimental,
compile-time opt-in** backend. `SWOFTY_EXPERIMENTAL_EA_FUEL=1 native/build.sh`
builds it for research; normal launcher builds keep it disabled. It detects
the EA executable by timestamp and image size, resolves the existing vehicle and
setter bindings, and scans the candidate availability pattern. It does not
change the Steam path or expose other car operations to EA. The availability
pointer, live car/ECU paths, car-built event timing, scope activation,
fuel-mode restoration and other lifecycle operations require independent
verification and in-game validation before it can be offered by default.

## Offline EA test, 2026-11-08

An isolated loader and profile ran the original Rich Fuel Mix 1.0.1 with the
experimental core on EA App 1.0.141.7865. In the first race the MFD unlocked,
but only one AI vehicle received the actual scales; the player selected Rich
while its active burn/power remained 1.0. A follow-up change retries active
fuel layers once per second on the EA path, outside replays. The test harness
also checks a late-arriving vehicle and a changed slot identity.

In the second offline race, all 21 vehicle ECUs held Rich burn 1.0869565 and
power 1.0425532. The player car (identified by its AI flag) reported Standard
mode with active multipliers 1.0/1.0, then after the player chose Rich reported
mode 2 with active burn 1.0869565, power 1.0425532, revs 1.0 and temperature
0.92. After the player selected Lean, the player ECU reported mode 0 with
active burn 0.9090909, power 0.9468085, revs 1.0 and temperature 1.10. After
selecting Standard, it reported mode 1 and all four active multipliers were
1.0. The MFD unlock entries were zero. These are live readings from the EA
game, not a simulated result or a Steam test.

Remaining: verify qualifying lap restoration, replays/flashbacks, session
changes, disabling the addon, and a Steam
regression; independently verify the candidate and register it with the
normal binding-generation workflow. The EA path remains opt-in until then.

The `ai-fix` paths mentioned in `ea.json` and `docs/reference/ea-bench.md` refer
to a separate Linux development bench; that folder is not present in this
checkout. Obtain its precise location or the relevant dump from its owner if
it is to be used for the binding work.
