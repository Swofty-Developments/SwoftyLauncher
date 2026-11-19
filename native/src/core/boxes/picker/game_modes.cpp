#include "core/boxes/picker/picker.hpp"

namespace era_picker {

const char *name_of(const void *object, size_t off)
{
    auto *holder = object ? at<const uint8_t *>(object, off) : nullptr;
    return holder ? at<const char *>(holder, 0x10) : nullptr;
}

uint8_t *setup_owner(void *mode)
{
    auto *owner = mode ? at<uint8_t *>(mode, MODE_SETUP) : nullptr;
    return owner ? at<uint8_t *>(owner, SETUP_MODES) : nullptr;
}

SetupModes setup_modes(void *mode)
{
    auto *modes = setup_owner(mode);
    if (!modes)
        return {};
    auto get = reinterpret_cast<uint8_t *(*)(void *)>((*reinterpret_cast<void ***>(modes))[SETUP_MODES_VFUNC / 8]);
    uint8_t *vec = get(modes);
    if (!vec)
        return {};
    return {at<uint8_t **>(vec, 8), at<uint8_t **>(vec, 0x10), true};
}

uint8_t *find_mode(void *mode, const std::string &want)
{
    if (want.empty())
        return nullptr;
    SetupModes modes = setup_modes(mode);
    if (!modes)
        return nullptr;
    for (auto **p = modes.begin; p != modes.end; ++p) {
        const char *name = name_of(*p, SETUP_MODE_NAME);
        if (name && want == name)
            return *p;
    }
    return nullptr;
}

// A weekend helper mode is listed in no game mode, so it is not in the game mode's list above: look it up in the
// store of every parsed setup mode (g_Game_Scenarios +0x40, logic::find_stored_mode). where says which list held it.
uint8_t *find_helper_mode(void *mode, const std::string &want, std::string &where)
{
    if (uint8_t *m = find_mode(mode, want)) {
        where = "the game mode's setup modes";
        return m;
    }
    uintptr_t slot = swofty::api ? swofty::api->resolve("g_Game_Scenarios") : 0, registry = 0;
    if (!slot || !mem::read(slot, registry) || !registry) {
        where = "the game mode's setup modes (the setup mode store needs g_Game_Scenarios)";
        return nullptr;
    }
    size_t seen = 0;
    std::vector<std::string> labels;
    uint8_t *m = logic::find_stored_mode(
        reinterpret_cast<const uint8_t *>(registry), want, [](const void *p, size_t n) { return mem::readable(p, n); }, &seen,
        &labels);
    if (m) {
        where = "the store of " + std::to_string(seen) + " parsed setup mode(s)";
        return m;
    }
    std::string all;
    for (auto &l : labels)
        all += (all.empty() ? "" : ", ") + l;
    where = "the game mode's setup modes and the store of " + std::to_string(seen) +
            " parsed setup mode(s): " + (all.empty() ? std::string("none") : all) +
            "; the game parses only rows with available true (the core sets it on a content.package's weekend_mode row at boot)";
    return m;
}

bool time_trial(void *mode)
{
    uintptr_t holder = 0, name = 0;
    std::string label;
    if (!mode || !mem::read(reinterpret_cast<uintptr_t>(mode) + MODE_SETUP, holder) || !holder ||
        !mem::read(holder + MODE_NAME, name) || !mem::c_string(name, label, 64))
        return false;
    return logic::time_trial_mode(label);
}

int64_t cars_of(void *mode, const std::string &list)
{
    int64_t cars = -1;
    for_each_host_setup(mode, [&](uint8_t *setup) {
        int64_t count = at<int64_t>(setup, LIST_COUNT);
        auto **lists = reinterpret_cast<uint8_t **>(setup + LISTS);
        for (int64_t i = 0; i < count && i < int64_t(LIST_SLOTS) && cars < 0; ++i) {
            const char *name = name_of(lists[i], LIST_NAME);
            uintptr_t begin = 0, end = 0;
            if (!name || strcmp(name, list.c_str()) ||
                !mem::read(reinterpret_cast<uintptr_t>(lists[i]) + logic::LIST_ENTRANTS_BEGIN, begin) ||
                !mem::read(reinterpret_cast<uintptr_t>(lists[i]) + logic::LIST_ENTRANTS_END, end))
                continue;
            cars = logic::entrant_count(begin, end);
        }
    });
    return cars;
}

logic::ListCars list_cars(const void *list)
{
    logic::ListCars out;
    uintptr_t holder = 0, name = 0, begin = 0, end = 0;
    auto at_list = reinterpret_cast<uintptr_t>(list);
    if (!list || !mem::read(at_list + LIST_NAME, holder) || !holder || !mem::read(holder + 0x10, name) ||
        !mem::c_string(name, out.name, 64))
        out.name.clear();
    if (list && mem::read(at_list + logic::LIST_ENTRANTS_BEGIN, begin) && mem::read(at_list + logic::LIST_ENTRANTS_END, end))
        out.cars = logic::entrant_count(begin, end);
    return out;
}

std::vector<logic::ListCars> lists_of(const uint8_t *entrant_setup, std::vector<void *> &ptrs)
{
    std::vector<logic::ListCars> out;
    int64_t count = 0;
    if (!entrant_setup || !mem::read(reinterpret_cast<uintptr_t>(entrant_setup) + LIST_COUNT, count) || count <= 0 ||
        count > int64_t(LIST_SLOTS))
        return out;
    for (int64_t i = 0; i < count; ++i) {
        uintptr_t list = 0;
        if (!mem::read(reinterpret_cast<uintptr_t>(entrant_setup) + LISTS + size_t(i) * 8, list))
            list = 0;
        ptrs.push_back(reinterpret_cast<void *>(list));
        out.push_back(list_cars(reinterpret_cast<void *>(list)));
    }
    return out;
}

}
