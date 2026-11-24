#pragma once
#include <cstdint>
#include <swofty/live_images.h>

// The grid rundown's driver portraits, read back from the game's render targets as the race load renders them, and
// each car's portrait reference from the view model leaf the game binds (portrait_data.hpp). The Live image service's
// table version 2 members and the Live table version 7 member answer from here.
namespace driver_portraits {

// The Live image service's table: the badge and tyre images (badge_images::api), and the portraits from version 2.
const SwoftyLiveImages *live_images();

int start(uint32_t flags);
bool owns(const char *reference);
int image(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision);
int image_full(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision);
int driver_portrait(uint32_t vehicle, char *reference, uint32_t cap);

void session_loading();
void session_unloaded();

}
