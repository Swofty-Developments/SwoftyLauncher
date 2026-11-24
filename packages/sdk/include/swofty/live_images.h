/* SPDX-License-Identifier: MIT */
#pragma once
#include <stdint.h>

/* Image access through SwoftyCoreLive table v4 images(). All results are copied into caller-owned
   memory. No image files or pointers into the game's resources are returned.
   start enables bounded, asynchronous readback of named runtime badge textures.
   image accepts team_badge_texture and tyre_icon_texture references, including catalogue and runtime badges.
   Tyre catalogue support is advertised by Live table version 5 (images table stays version 1).
   Catalogue reads and decoding run on a worker only when requested, following active redirects.
   LATE means an archive read is pending; UNAVAILABLE means no usable image.
   sources returns the active game-relative archive redirects, not a cache search.
   image returns RGBA8 bytes with alpha, at most 64x64; caller provides 16,384 bytes.
   Check the table version before accessing its members. Older cores may omit it.
   Version 2 appends start_portraits and image_full for the grid rundown's driver portraits (Live table version 7,
   driver_portrait). start_portraits(0) arms their capture; call it before the race loads (at addon init), since the game
   renders each portrait once, during the race load, and only when the grid rundown is queued (a race session with
   more than 10 entrants and Game.SkipGridRundown off). flags must be 0. A portrait reference is the render target's
   name, "eaid://game_rtt_tex_NNNN". image gives it scaled to at most 64x64 like any other reference; image_full gives
   it at its rendered size (600x800 for the rundown, at most 1024 on a side, so 4,194,304 bytes always suffice): with a
   short cap or a NULL rgba it writes width and height and answers SWOFTY_ASSET_INVALID, so a caller can size its
   buffer. Portrait pixels stay until the session unloads; clear leaves them. */
typedef struct SwoftyLiveImages
{
    uint32_t version;
    int (*start)(void);
    int (*sources)(char *json, uint32_t cap, uint64_t *revision);
    int (*image)(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height,
                 uint64_t *revision);
    void (*clear)(void); /* explicit cache reset: retire pixels; queued GPU work stays alive until its fence */
    int (*start_portraits)(uint32_t flags);
    int (*image_full)(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height,
                      uint64_t *revision);
} SwoftyLiveImages;

#define SWOFTY_LIVE_PORTRAIT_MAX_SIDE 1024
