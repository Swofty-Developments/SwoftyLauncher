---
title: Diagnostics follow-ups
---

# Diagnostics follow-ups

Send Logs now carries the GPU, driver, video memory and RAM facts, the last five crash reports and DRED breadcrumbs from the first GPU hang. These parts need game hooks that core does not have, so none exists yet.

## Per-car load line on the select screen

One `loader.log` line for each car or mesh load while a season box is open: the `.erp` file name, the slot, the mesh and record count, the triangle count, and whether a raytracing acceleration structure build was requested. DRED shows the hang at the fourth `BUILDRAYTRACINGACCELERATIONSTRUCTURE` of a flush with no way to tie it to a car. Core has no hook on the game's acceleration structure builds or its raytracing validation messages, and the bindings carry no `Render_OnRaytracingValidationMsg`. The work is to locate the build call and the mesh loader in `re/`, add both as bindings, and log from them.

## Session end classification

The qualifying or race result with positions and lap times, and the grid. The core wraps `Sess_Record_GroupTimes` and `Sess_Build_GroupResult` to rewrite grids, but nothing reads the finished result for the log. The work is a read-only reader over the group result rows described in the sessions page.

## Menu input events

Hover and click target ids on the front end. Core has no hook on the menu input path; `loader.log` records screen and flow events only.
