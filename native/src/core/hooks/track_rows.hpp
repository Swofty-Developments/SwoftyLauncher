#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

#include <swofty_api.h>

namespace swofty_core::hooks::track_rows {

// The row vectors the two track select screens fill from #track_data.track_list, one row per displayed track.
//
// Grand Prix (Trk_GrandPrixTrackSelectVm_RefreshTrackList 0x141c05600): rows are 0x128 bytes in the vector
// {vm+0x750 begin, vm+0x758 end}. PROVEN: the refresh clears it with end = begin (0x141c05644 mov [rbx+0x758],rax after
// mov rax,[rbx+0x750]) and Trk_GrandPrixTrackSelectVm_InsertRoundRow's companion 0x141c09f70 bounds-checks a row index
// as (end-begin)>>3 times 0x14c1bacf914c1bad (the inverse of 0x25) against rdx (0x141c09f83 to 0x141c09fa6), then
// addresses a row as imul r12,rdx,0x128 (0x141c09fba).
//
// Championship settings (Trk_SettingsSelectTrackTabVm_RefreshTrackList 0x141de65e0): rows are 0x168 bytes in the vector
// {obj+0x90 begin, obj+0x98 end} of obj = [vm+0xb28]. PROVEN: the refresh clears it with end = begin (0x141de6623 to
// 0x141de662a on rdi = [rsi+0xb28]) and InsertRoundNode 0x141de74b0 counts it as (end-begin)>>3 times 0xa4fa4fa5, the
// 32-bit inverse of 0x2d (0x141de74e9 to 0x141de7502), and 0x2d * 8 = 0x168.
//
// NOT PROVEN, so left out: any row field. A row holds view-model property handles and the node reference to the
// track's TrackData node (the GP loop passes the entry's node handle [entry+0x2c0]+8 to 0x141c09f70), not a string, so
// the track id as the registry names it needs a node value read this core has not proven. The table therefore counts
// rows and offers get, remove and insert as null.
inline constexpr size_t GP_BEGIN = 0x750, GP_END = 0x758, GP_ROW = 0x128;
inline constexpr size_t SETTINGS_OWNER = 0xb28, SETTINGS_BEGIN = 0x90, SETTINGS_END = 0x98, SETTINGS_ROW = 0x168;

enum class Screen { grand_prix, settings };

inline bool word(const uint8_t *base, size_t off, uintptr_t &out)
{
    if (!base)
        return false;
    std::memcpy(&out, base + off, sizeof out);
    return true;
}

inline int rows_of(const void *vm, Screen screen)
{
    auto *v = static_cast<const uint8_t *>(vm);
    uintptr_t begin = 0, end = 0;
    size_t row = GP_ROW;
    if (screen == Screen::grand_prix) {
        if (!word(v, GP_BEGIN, begin) || !word(v, GP_END, end))
            return 0;
    } else {
        uintptr_t owner = 0;
        if (!word(v, SETTINGS_OWNER, owner) || !owner)
            return 0;
        auto *o = reinterpret_cast<const uint8_t *>(owner);
        if (!word(o, SETTINGS_BEGIN, begin) || !word(o, SETTINGS_END, end))
            return 0;
        row = SETTINGS_ROW;
    }
    if (!begin || end < begin)
        return 0;
    size_t n = size_t(end - begin) / row;
    return n > 4096 ? 0 : int(n);
}

class Table {
public:
    Table(const void *vm, Screen screen) : vm_(vm), screen_(screen)
    {
        table_ = {sizeof(SwoftyTrackList), SWOFTY_TRACK_LIST_VERSION, this, &count, nullptr, nullptr, nullptr};
    }

    const SwoftyTrackList *table() const { return &table_; }

private:
    static int count(void *self)
    {
        auto *t = static_cast<Table *>(self);
        return rows_of(t->vm_, t->screen_);
    }

    const void *vm_;
    Screen screen_;
    SwoftyTrackList table_{};
};

}
