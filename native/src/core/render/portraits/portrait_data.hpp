#pragma once
#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

// The driver portraits the race load renders for the grid rundown, as the game names and binds them.
//
// The render target's one constructor (KB 0x1434f2390, Steam 0x1434f4920) takes a free slot of a 1000-slot pool and
// names the texture "eaid://game_rtt_tex_%04d", with the views "<name>.srv" and "<name>_srgb.srv"; one mip, one layer.
// The thumbnail system's completion callback (KB 0x142e32310) writes the view model leaf
// "<driver>.textures.<set>.Thumbnail" to one of the two view names and sets "<driver>.textures.has_texture". The
// driver node is a list element of a root list picked by the low byte of the packed driver id (KB 0x14c972b50):
// 0 preloaded_drivers, 1 custom_drivers, 2 network_drivers, indexed by its high 16 bits. The race load
// (Sess_Loader_RequestLoadThumbnails, KB 0x142d4d040) asks for one portrait per grid slot, GridRundownLeft and
// GridRundownRight in turn (thumbnails.thumbnails ThumbnailSet: camera rtt_Driver_Default, pose
// DriverRTTPoseGenericLookForwards, 600x800); Broadcast is the 400x400 head shot other screens ask for.
namespace render::portraits::data {

inline constexpr unsigned MAX_SIDE = 1024, CACHE = 40, PENDING = 8, CAPTURES_PER_NAME = 8;
inline constexpr std::string_view PREFIX = "eaid://game_rtt_tex_";
inline constexpr size_t DIGITS = 4;
inline constexpr std::array<const char *, 3> DRIVER_LISTS{"preloaded_drivers", "custom_drivers", "network_drivers"};
inline constexpr std::array<const char *, 3> SETS{"GridRundownLeft", "GridRundownRight", "Broadcast"};

struct Shape {
    unsigned width, height;
};
inline constexpr std::array<Shape, 2> GAME_SHAPES{{{600, 800}, {400, 400}}};

// The pool name inside a D3D debug name or a view model value: "eaid://game_rtt_tex_0012" for any of
// "eaid://game_rtt_tex_0012", its ".srv", "_srgb.srv" and ".image" forms, or a path that ends in one of them.
inline std::optional<std::string> resource(std::string_view name, bool *srgb = nullptr)
{
    const auto at = name.find(PREFIX);
    if (at == std::string_view::npos || (at && name[at - 1] != '/' && name[at - 1] != ' '))
        return {};
    auto rest = name.substr(at + PREFIX.size());
    if (rest.size() < DIGITS)
        return {};
    for (size_t i = 0; i < DIGITS; i++)
        if (rest[i] < '0' || rest[i] > '9')
            return {};
    const auto suffix = rest.substr(DIGITS);
    const bool is_srgb = suffix == "_srgb.srv";
    if (!suffix.empty() && suffix != ".srv" && !is_srgb && suffix != ".image")
        return {};
    if (srgb)
        *srgb = is_srgb;
    return std::string(name.substr(at, PREFIX.size() + DIGITS));
}

// The view model path of one driver's thumbnail leaf, as the walk's segments: list, index, "textures", set, "Thumbnail".
struct Path {
    const char *list = nullptr;
    uint32_t index = 0;
    const char *set = nullptr;
};

inline std::optional<Path> path(uint8_t domain, uint16_t index, size_t set)
{
    if (domain >= DRIVER_LISTS.size() || index >= 0x3fff || set >= SETS.size())
        return {};
    return Path{DRIVER_LISTS[domain], index, SETS[set]};
}

inline bool game_shape(unsigned width, unsigned height)
{
    for (const Shape &s : GAME_SHAPES)
        if (s.width == width && s.height == height)
            return true;
    return false;
}

// DXGI formats by value, so the host test needs no D3D header: R8G8B8A8 TYPELESS/UNORM/UNORM_SRGB (27-29) and
// B8G8R8A8 UNORM (87), TYPELESS (90) and UNORM_SRGB (91).
inline bool format(unsigned dxgi, bool &bgra)
{
    bgra = dxgi == 87 || dxgi == 90 || dxgi == 91;
    return bgra || (dxgi >= 27 && dxgi <= 29);
}

}
