#pragma once

namespace ce {

inline int clamp_extra(int extra, int base)
{
    int floor = base > 0 ? -base : 0;
    return extra < floor ? floor : extra;
}

inline int merge_extra(int game_extra, int adjustment, int base)
{
    return clamp_extra(game_extra + adjustment, base);
}

inline int shown_points(int points) { return points < 0 ? 0 : points; }

}
