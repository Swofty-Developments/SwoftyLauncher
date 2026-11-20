# ui: authored UI formats + packaging + binding runtime (menu edit/add)

Owner: see `subsystems.tsv` (prefix `Ui`).

This lane owns the **format reference**, not a slice of the exe. The full write-up is
[`ui_system.md`](ui_system.md): the `!pkg` binary container, the `.ui`/UIDF node grammar,
`.uidb`/`.uir`, the `.idf` scene-layout resource, the ERP UI archive layout, the view-model
binding runtime, and the recipes to edit or add a widget/box/tile/screen. Since 2026-10-23 it also
states what the engine's loader requires of each node slot (§2.1–2.6), how templates register and
instantiate (§2.9), the eXRef `xref` rule (§5.5) and what a rejection takes down and what `check.rs`
enforces (§8.4); the engine-side data flow behind those facts is in [`vm.md`](vm.md) (`#templates`,
`#tabs`, `#element-init`, `#layout-attach`, `#signals`, `#failures`, `#trace`).

Related exe lanes (functions/globals live there, not here): `vm` (node-class registry, template store
and instantiation, element and behaviour inits, signals, binding,
routes, screen loader), `fe_flow` (flow state machine, container tab builders), `fe_home`
(home tile menu), `fe_tiles` (generic tile builder), `fe_popup`, `fe_input`, `hud`, `assets`
(eaid:// resolution), `career` (the R&D box-grid / pie / upgrade-tile surfaces this reference
cross-references).

Primary sources this reference is distilled from: the launcher's own UI generator
(`/mnt/work/VSC/F1ModManager/packages/core/src/games/f1_25/ui/{node,pages,patch,check,mod}.rs`), the binary
format crates (`packages/formats/src/{pkg,erp}.rs`), and the native runtime loader/binder
(`native/src/core/frontend/{ui,ui_vm}.cpp`).

## Inbox
