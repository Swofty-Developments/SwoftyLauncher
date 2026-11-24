---
title: Native Chart options
description: Add-on-controlled text sizes, chart surfaces and borders, grids, rounded data blocks, chart images and line widths.
---

# Native Chart options

Check for both `view.chart` and `view.chart.options` in `hello.params.capabilities` before sending these extensions. A missing capability list is empty; API version stays 1. These are settings the add-on supplies with each chart, not new launcher settings or chart interaction controls.

Omitted fields retain the existing chart appearance and JSON shape. Existing table images, tables, cards and launcher pages are unchanged. Rendering is native: no chart image files, browser, web service or chart cache files are generated.

## Text sizes

`chart.options.text` has independent optional sizes for `title`, `caption`, `x_title`, `y_title`, `x_ticks`, `y_ticks`, `legend` and `labels` (range/heatmap cell text). Sizes use launcher-scaled logical pixels and range from 6 to 64.

Defaults remain 14 for the title; 11 for caption, legend and x-axis title; 10 for y-axis title, axis values and cell text. Plot margins expand when larger text or axis images need room. Crowded ticks are thinned; cell labels are omitted when they do not fit. This changes no font family or launcher typography.

## Chart background, plot area and border

```json
"options": {
  "background": "#171A27",
  "plot_background": "#10121C",
  "border": { "color": "#9F7AEA", "width": 2 },
  "corner_radius": 14,
  "text": { "title": 20, "x_ticks": 12, "y_ticks": 14, "labels": 13 }
}
```

Colours accept `#RRGGBB` or `#RRGGBBAA`. The outer chart still defaults to the launcher surface, its existing 1 px border and 10 px corners. The plot has no extra background by default. Border width is 0–16 (0 hides it); chart and block corner radii are 0–64, clamped to the available rectangle.

## Grid appearance

`options.x_grid` controls vertical grid lines; `options.y_grid` controls horizontal grid lines. Both accept optional `visible`, `color`, `width` and `dash`:

```json
"x_grid": { "visible": false },
"y_grid": { "color": "#384156", "width": 2, "dash": [8,4,2,4] }
```

Defaults remain solid lines in the launcher grid colour, width 1.6. A width of 0 hides the lines. Dash lengths alternate on/off in logical pixels: an even number of 1–100 px lengths, at most 16. An omitted/empty pattern is solid. Axes, numeric domains and data coordinates retain their existing meaning.

## Rounded block styling

`series.blocks` styles bar bodies, box bodies, ranges and heatmap cells. A range/heatmap `cell.style` overrides its series block style. Style fields are `radius` (default 0), optional `fill` and optional `border: {color,width}`:

```json
{ "name":"Stints", "type":"ranges", "color":"#20D6BB",
  "blocks": { "radius":7, "fill":"#20D6BB44", "border":{"color":"#20D6BB","width":2} },
  "cells":[ { "x":[0,12], "y":[-0.3,0.3], "label":"12 laps" } ] }
```

Without block styling, the original square data marks are retained. A fill defaults to the cell/series colour. Radii do not change interval endpoints or recorded durations. Styling a box body does not change its supplied quartiles, median or whiskers.

## Thicker lines

`series.line_width` sets the width of line-series strokes and box whiskers/median lines, from 0.5 to 16 logical pixels. Omit it for the existing 1.6 width. Existing `dashed` and `step_after` behaviour remains available; missing values still break a line. This is independent of `bars.width` and `boxes.width`, which remain category-axis dimensions.

## Images within charts

`chart.images` accepts at most 64 placements. Each has the same `image` (allowed file path or sized object) and/or `image_data` (in-memory PNG) description as [table images](./tables#pictures-supplied-in-memory), plus a location:

| Location | Fields | Placement |
|---|---|---|
| `"at":"plot"` | `x`, `y` | Image centred at a data coordinate, clipped to the plot |
| `"at":"cell"` | `series`, `cell` | Image beside the label within that range/heatmap block |
| `"at":"x_tick"` | `tick` | Image beside the explicit x-axis tick's label |
| `"at":"y_tick"` | `tick` | Image beside the explicit y-axis tick's label |
| `"at":"legend"` | `series` | Image in place of that series' legend swatch, beside its name |

Indices are zero-based. A cell, tick or legend item accepts one image; plot placements can overlap in input order. Tick images require explicit ticks. Empty-name series still have no legend entry. Image positions do not expand inferred domains; give a chart explicit domains if it contains only plot images.

```json
"images": [
  { "at":"cell", "series":0, "cell":0,
    "image_data":{"png":"<base64 PNG>","width":20,"height":20,"fit":"contain","transparent":true} },
  { "at":"plot", "x":12, "y":1,
    "image":{"path":"/home/you/.local/share/swofty/cache/addons/your.addon/logo.png","width":24,"height":24,"fit":"contain","transparent":true} }
]
```

Default image boxes are 16 by 16 logical pixels; explicit dimensions are 1–512. Cell images shrink to their available block. `contain` preserves the whole image and aspect ratio; `cover` fills/crops its box. `transparent:true` omits the image box's background.

Valid `image_data` takes precedence over `image`. In-memory PNG decoding uses the same 64 KiB encoded/256-by-256 static-image limits, bounded cache and image retirement as table images. Invalid PNG data can fall back to an allowed file, then to the remaining text. Files must be under that add-on's data/cache roots, using the same checks and asynchronous asset loading as table images. No network image URLs or raw Live State texture references are accepted here. The add-on supplies image pixels/paths; the chart renderer does not resolve game assets.

## Example and checks

`swofty-launcher --chart-demo` has a **Chart options** tab demonstrating the seven additions. Its checker images are synthetic fixture data, not game assets. The existing gallery tabs retain their defaults. The sample button replaces the add-on-supplied settings; it introduces no user styling settings in the launcher.

`cargo build -p swofty-addon --example native_chart_options` builds a JSON-lines example which checks both capabilities and strips the new fields for a chart-capable older launcher. `tools/native-charts/options-smoke.py` checks its protocol, fallback and absence of generated files.
