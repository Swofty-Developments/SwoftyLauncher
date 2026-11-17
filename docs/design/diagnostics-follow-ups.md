---
title: Diagnostics follow-ups
---

# Diagnostics follow-ups

Send Logs now carries the GPU, driver, video memory and RAM facts, the last five crash reports and DRED breadcrumbs from the first GPU hang. These parts need game hooks that core does not have, so none exists yet.

## Per-car load line on the select screen

Core names each bottom-level ray tracing build in `loader.log` the first time it sees that structure with that shape, up to 800 builds: `[gfx] rtas build <n> on list 0x<list> into 0x<dest>` with each geometry's vertex and index counts (`v<vertices>/i<indices>`) and formats. The DRED section of a crash report gives each command list node its address (`command list <name> (0x<list>)`), so the build a hang stops at is the one on that list, and its index counts match the records of the car mesh that holds it. Still missing: the `.erp` file name and slot of each mesh load, which needs the game's mesh loader located in `re/` and added as a binding.

## Session end classification

The qualifying or race result with positions and lap times, and the grid. The core wraps `Sess_Record_GroupTimes` and `Sess_Build_GroupResult` to rewrite grids, but nothing reads the finished result for the log. The work is a read-only reader over the group result rows described in the sessions page.

## Menu input events

Hover and click target ids on the front end. Core has no hook on the menu input path; `loader.log` records screen and flow events only.
