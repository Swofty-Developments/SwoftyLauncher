---
title: Addon Browser
description: Finding, installing and starring community addons in the launcher's Addon Browser - search, categories, sorting, filters, creators, updates and publishing your own.
---

# Addon Browser

The **Addon Browser** page lists the addons creators publish to Swofty Launcher. Every version is reviewed before it's listed.

## Find an addon

The page opens on **All**: one row per category, in the order of the category bar (Seasons, Tracks, Cars, Helmets, Shaders, Career, AI, Performance, Utility, Other). A category with no addons has no row. Each row shows the category's addon cards in the chosen sort order, up to 24, with its count at the top.

- Scroll a row sideways with the mouse wheel while holding Shift, with a trackpad swipe, or with the arrow buttons at the top right of the row. The arrows move by whole cards and stop at the ends. A fade at the edge of a row shows there are more cards past it.
- **See all** opens that category's addons as a grid. A row with more than 24 addons ends in a **See all** card.

Above the rows, the category bar switches between All and each category. Searching, choosing a filter or picking a category replaces the rows with a grid of the matching addons.

- **Search:** matches names, ids, creators, categories, types and descriptions, and tolerates one typo in longer words. `author:<name>` and `category:<name>` narrow it down. Creators that match show above the addons. A search from All covers every category, and a search inside a category covers that category.
- **Sort:** Most stars, Most downloads, Trending (downloads in the last 30 days), Recently updated, Newest and Name. While you search, the order is Best match.
- **Filters:** type, state (installed, not installed, update available, starred by you, works with this launcher), creator, and **Hide addons that run code**. A filter chosen on All applies to every category.

Select an addon for its description, a 30-day downloads chart, its file size and checksum, the launcher version it needs, and every version's changelog. When the creator lists the addon's source code, a **Source code** button opens it; an addon without one has no button, and an installed addon's page shows **Not listed** under **Source code**. **More from** lists the same creator's other addons.

## Install one

Select **Install**. The launcher queues the newest version that works with your launcher, checks its size and SHA-256, and installs it switched off in the profile you selected. A download card slides up above the Play bar while the install runs. Select it to open **Downloads**. An addon marked **Runs code in the game** includes native code that runs inside F1 25.

## Downloads {#downloads}

The **Downloads** page lists Addon Browser installs and updates, including **Update addons** jobs. Each row shows its status and any error.

The download card sits at the bottom right of every page while a job is queued or active. It names the addon being installed, shows its progress and adds **+N more** when other jobs wait. It slides away a few seconds after the queue empties and stays hidden on the **Downloads** page. A failed install keeps the card visible in a warning colour until you select **X** or open **Downloads**. There is no Downloads entry in the sidebar; open the page from the card or from the links in the Addon Browser.

- **Concurrent jobs** sets how many installs can run at once, from 1 to 10. The default is 2. An install occupies a slot while it downloads its required addons, verifies files and installs them. Changing the limit does not interrupt an active job.
- **Remove** takes a queued job out of the queue. It cannot stop an active install.
- **Retry** starts a failed job again. If its version is no longer available, **Request current version** queues the current compatible version instead.
- **Clear finished** removes completed and failed rows. It leaves queued and active jobs alone.

The launcher remembers the concurrency limit, but clears the queue and history when it closes. If you close it while jobs are pending, it warns that active work will be interrupted and queued jobs lost. Launcher updates and other downloads do not appear on this page.

A job keeps the profile it was requested from, even if you switch profiles while it waits. If that profile is deleted, the launcher leaves the installed addon out of other profiles and notes this in the result.

### Requirements {#requirements}

An addon can require other addons (its manifest's [`requires`](/reference/addons/manifest#requires-conflicts-load-after)). Installing it installs them too, first:

- A required addon you have at a matching version, or one built into the launcher, stays as it is.
- A required addon you don't have, or have at a version that doesn't match, comes from the Addon Browser: the newest version that matches and works with your launcher. A required addon's own requirements install the same way.
- When a required addon isn't on the Addon Browser, no version of it matches, or replacing yours would break another addon you have, the launcher refuses the install and names the unmet requirements. If an install fails after a required addon was installed, that addon stays installed; **Downloads** lists it in the failed result.

The addon's page lists **Also installs** with the other addons the install brings, and its card shows the same line. **Downloads** shows the current requirement and progress, and **Activity** records what else was installed. Each version in **Versions** lists what it requires.

### If a download fails {#if-a-download-fails}

When the connection drops or breaks partway through a download, the launcher waits a few seconds and asks for the rest of the file. It gives up after four tries in a row that bring no new data, or twelve tries in all. The finished file is still checked against its size and SHA-256.

When it keeps breaking, the install fails with **the connection kept breaking during the download**. Antivirus HTTPS or web scanning, a VPN or a proxy usually causes it:

1. Turn off your antivirus's HTTPS or web scanning, or add an exclusion for Swofty Launcher.
2. Turn off any VPN or proxy.
3. Select **Retry** on Downloads. If it still fails, try another network, such as a phone hotspot.

The error's second line holds the network error, and `state/registry.log` in the data folder keeps it.

## Updates

When an installed addon that is on in your active profile has a newer version, **Play** becomes **Update addons** with the number of updates. Updates for addons that are off are listed here and on the Addons page without changing **Play**. See [Updating](/using/updating#addon-updates).

## Stars

Select the star on an addon to star it. The launcher keeps your stars and syncs them with the registry, so they follow you to another network. Each person counts once per addon.

## Creators

Select a creator's name, or open **Creators**, for their page: their bio and links, their totals, and every addon they've published.

## Publish your own

Creators with the **Swofty Launcher Addon Creator** role on the [Swofty Discord](https://discord.swofty.net) publish at [publish.f1.swofty.net](https://publish.f1.swofty.net). Each version goes to review first, and appears in the Addon Browser once it's approved.

## Activity and offline use

**Activity** lists the last 50 catalog refreshes, star syncs, installs and updates, with their results. Offline, the page shows the addons from the last refresh and says when that was. Everything it does is also written to `state/registry.log` in the data folder, which **Send Logs to Developers** includes.
