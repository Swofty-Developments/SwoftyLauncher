#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <swofty_api.h>
#include <swofty/live_images.h>
#include <cassert>
#include <cstddef>
#include <cstdio>

static_assert(offsetof(SwoftyLive, tyre_icon_texture) == offsetof(SwoftyLive, images) + sizeof(void *));

int main(int argc, char **argv)
{
    assert(argc == 2);
    HMODULE core = LoadLibraryA(argv[1]);
    assert(core);
    auto get = reinterpret_cast<SwoftyCoreLiveFn>(GetProcAddress(core, "SwoftyCoreLive"));
    assert(get);
    const auto *live = get();
    assert(live && live->version == 5 && live->tyre_icon_texture && live->images);
    const auto *images = live->images();
    assert(images && images->version == 1 && images->image);
    char path[160] = "stale";
    assert(live->tyre_icon_texture(0, nullptr, sizeof path) == SWOFTY_ASSET_INVALID);
    assert(live->tyre_icon_texture(SWOFTY_LIVE_MAX_CARS, path, sizeof path) == SWOFTY_ASSET_INVALID);
    assert(live->tyre_icon_texture(0, path, sizeof path) == SWOFTY_ASSET_UNAVAILABLE && !path[0]);
    assert(!live->available());
    assert(FreeLibrary(core));
    std::puts("PASS: candidate core exports Live v5, preserves images v1 and rejects invalid/unavailable tyre queries");
}
