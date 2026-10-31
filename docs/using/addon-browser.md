---
title: Addon Browser
description: Finding, installing and starring community addons in the launcher's Addon Browser - search, categories, sorting, filters, creators, updates and publishing your own.
---

# Addon Browser

The **Addon Browser** page lists the addons creators publish to Swofty Launcher. Every version is reviewed before it's listed.

## Find an addon

The page opens on a tile for each category: Seasons, Tracks, Cars, Helmets, Shaders, Career, AI, Performance, Utility and Other. Each tile shows how many addons the category holds and the icons of its most starred addons. Other holds the addons without a category. A category with no addons yet shows **No addons yet**.

Every addon is listed under the tiles, with its count and the sort order. Select a tile for that category's addons. Above the addons, the category bar switches between All and each category, and **Categories** goes back to the tiles and clears the search and filters.

- **Search:** matches names, ids, creators, categories, types and descriptions, and tolerates one typo in longer words. `author:<name>` and `category:<name>` narrow it down. Creators that match show above the addons. A search from the tiles covers every category, and a search inside a category covers that category.
- **Sort:** Most stars, Most downloads, Trending (downloads in the last 30 days), Recently updated, Newest and Name. While you search, the order is Best match.
- **Filters:** type, state (installed, not installed, update available, starred by you, works with this launcher), creator, and **Hide addons that run code**. A filter chosen on the tiles applies to every category.

Select an addon for its description, a 30-day downloads chart, its file size and checksum, the launcher version it needs, and every version's changelog. **More from** lists the same creator's other addons.

## Install one

Select **Install**. The launcher downloads the newest version that works with your launcher, checks its size and SHA-256, installs it switched off, and opens the **Addons** page with it highlighted. An addon marked **Runs code in the game** includes native code that runs inside F1 25.

### Requirements {#requirements}

An addon can require other addons (its manifest's [`requires`](/reference/addons/manifest#requires-conflicts-load-after)). Installing it installs them too, first:

- A required addon you have at a matching version, or one built into the launcher, stays as it is.
- A required addon you don't have, or have at a version that doesn't match, comes from the Addon Browser: the newest version that matches and works with your launcher. A required addon's own requirements install the same way.
- When a required addon isn't on the Addon Browser, no version of it matches, or replacing yours would break another addon you have, nothing installs. The addon shows **Missing requirements**, naming each missing addon and the version it needs.

The addon's page lists **Also installs** with the other addons the install brings, and its card shows the same line. The progress bar covers every download, and **Activity** records what else was installed. Each version in **Versions** lists what it requires.

### If a download fails {#if-a-download-fails}

When the connection drops or breaks partway through a download, the launcher waits a few seconds and asks for the rest of the file. It gives up after four tries in a row that bring no new data, or twelve tries in all. The finished file is still checked against its size and SHA-256.

When it keeps breaking, the install fails with **the connection kept breaking during the download**. Antivirus HTTPS or web scanning, a VPN or a proxy usually causes it:

1. Turn off your antivirus's HTTPS or web scanning, or add an exclusion for Swofty Launcher.
2. Turn off any VPN or proxy.
3. Install again. If it still fails, try another network, such as a phone hotspot.

The error's second line holds the network error, and `state/registry.log` in the data folder keeps it.

## Updates

When an installed addon has a newer version, **Play** becomes **Update addons** with the number of updates. See [Updating](/using/updating#addon-updates).

## Stars

Select the star on an addon to star it. The launcher keeps your stars and syncs them with the registry, so they follow you to another network. Each person counts once per addon.

## Creators

Select a creator's name, or open **Creators**, for their page: their bio and links, their totals, and every addon they've published.

## Publish your own

Creators with the **Swofty Launcher Addon Creator** role on the [Swofty Discord](https://discord.swofty.net) publish at [publish.f1.swofty.net](https://publish.f1.swofty.net). Each version goes to review first, and appears in the Addon Browser once it's approved.

## Activity and offline use

**Activity** lists the last 50 catalog refreshes, star syncs, installs and updates, with their results. Offline, the page shows the addons from the last refresh and says when that was. Everything it does is also written to `state/registry.log` in the data folder, which **Send Logs to Developers** includes.
