#include "core/boxes/picker/picker.hpp"

#include <algorithm>
#include <cstdio>

#include "core/apply/moments.hpp"

namespace era_picker {

bool skip_once(const std::string &key)
{
    std::lock_guard<std::mutex> g(picker.lock);
    if (std::find(picker.skipped.begin(), picker.skipped.end(), key) != picker.skipped.end())
        return false;
    picker.skipped.push_back(key);
    return true;
}

bool is_ours(const char *list)
{
    return list && std::find(picker.built.lists.begin(), picker.built.lists.end(), list) != picker.built.lists.end();
}

bool renarrow(void *mode)
{
    auto *modes = setup_owner(mode);
    if (!modes)
        return false;
    const char *selected = name_of(at<void *>(modes, SELECTED_LIST), LIST_NAME);
    std::string list = is_ours(selected) ? selected : selected ? std::string() : picker.last_pick;
    if (list.empty() || !is_ours(list.c_str()))
        return false;
    narrow(mode, list.c_str());
    if (picker.lines.renarrow.note())
        swofty::log("[era] picker built with %s still selected (%s): %s narrowed to it again", list.c_str(),
                    selected ? "the setup's selected list" : "the last addon box pressed", logic::HOST);
    return true;
}

void *registry() { return *reinterpret_cast<void **>(b.registry); }

uint8_t *addon_list(uint8_t *vm)
{
    auto *root = at<uint8_t *>(vm, VM_ROOT);
    if (!root)
        return nullptr;
    uint64_t key = logic::ADDON_TILES;
    auto **h = reinterpret_cast<Vm_GetOrCreate_ListChild_t *>(b.list_child)(registry(), root + 8,
                                                                            reinterpret_cast<const uint32_t *>(&key));
    return h ? static_cast<uint8_t *>(*h) : nullptr;
}

uint8_t *addon_selection(uint8_t *vm)
{
    auto *root = at<uint8_t *>(vm, VM_ROOT);
    if (!root)
        return nullptr;
    Handle empty{};
    auto **h = reinterpret_cast<Vm_Set_NodeRef_t *>(b.node_ref)(registry(), root + 8, logic::SELECTED_ADDON_TILE, &empty, 1);
    return h ? static_cast<uint8_t *>(*h) : nullptr;
}

bool reset_set(uint8_t *vm, uint8_t *list)
{
    if ((picker.set.list.node || picker.set.selected.node) && (picker.set.list.node != list || picker.built.vm != vm) &&
        picker.lines.detached.note())
        swofty::log("[era] detached the addon row from a previous view model (its boxes there cleared, its two nodes "
                    "unlinked and released)");
    logic::relink(picker.set, list, [](Set &s) { reinterpret_cast<FeTile_Clear_EraTileSet_Thunk_t *>(b.clear)(&s); });
    uint8_t *selected = addon_selection(vm);
    if (!selected)
        return false;
    logic::link(picker.set.selected, selected);
    return true;
}

void site_entry(SwoftyContext *c)
{
    auto *vm = reinterpret_cast<uint8_t *>(c->rsi);
    if (!vm || !picker.live || picker.off)
        return;
    restore_lists();
    void *mode = at<void *>(vm, VM_MODE);
    void *alloc = at<void *>(vm, VM_ALLOCATOR);
    auto *begin = at<uint8_t *>(vm, VM_TILE_BEGIN);
    auto *end = at<uint8_t *>(vm, VM_TILE_END);
    size_t stock = end > begin ? size_t(end - begin) / TILE_ENTRY : 0;
    uint8_t *list = addon_list(vm);
    if (!list || !reset_set(vm, list)) {
        put_back_drs();
        picker.built = {};
        swofty::log("[era] the view model's addon_tiles or selected_addon_tile node could not be made; no addon row");
        return;
    }
    std::vector<logic::Box> order;
    {
        std::lock_guard<std::mutex> g(picker.lock);
        order = logic::ordered(picker.boxes);
    }
    auto available = reinterpret_cast<FeTile_Is_EraScenarioAvailable_Thunk_t *>(b.available);
    auto add = reinterpret_cast<FeTile_Add_EraTile_t *>(b.add);
    uint8_t *tiles_node = at<uint8_t *>(vm, VM_TILES_NODE);
    bool tt = time_trial(mode);
    picker.built = {vm, stock, 0, {}};
    ref<uint8_t *>(vm, VM_TILES_NODE) = list;
    for (const auto &box : order) {
        if (picker.built.boxes >= logic::MAX_BOXES)
            break;
        if (!logic::shown(box, tt))
            continue;
        if (!mode || !available(mode, logic::HOST, pressed_list(box.entrant_list.c_str()).c_str())) {
            if (skip_once(box.entrant_list))
                swofty::log("[era] box %s from %s skipped: this screen's mode does not offer the entrant list under %s "
                            "(it must exist and be in %s's entrant lists)",
                            box.entrant_list.c_str(), box.owner.c_str(), logic::HOST, logic::HOST);
            continue;
        }
        const std::string &list = logic::list_for(box, tt);
        if (tt && list != box.entrant_list && !available(mode, logic::HOST, pressed_list(list.c_str()).c_str())) {
            if (skip_once("time_trial " + box.entrant_list))
                swofty::log("[era] box %s from %s skipped in Time Trial: its Time Trial entrant list %s is not offered "
                            "under %s (it must exist and be in %s's entrant lists)",
                            box.entrant_list.c_str(), box.owner.c_str(), list.c_str(), logic::HOST, logic::HOST);
            continue;
        }
        if (tt) {
            int64_t cars = cars_of(mode, list);
            std::string why = logic::time_trial_refusal(box, cars);
            if (!why.empty()) {
                if (skip_once("time_trial " + box.entrant_list))
                    swofty::log("[era] box %s from %s skipped in Time Trial: %s", box.entrant_list.c_str(), box.owner.c_str(),
                                why.c_str());
                continue;
            }
            if (cars < 0 && skip_once("time_trial count " + list))
                swofty::log("[era] box %s from %s: the car count of its Time Trial list %s could not be read; not checked",
                            box.entrant_list.c_str(), box.owner.c_str(), list.c_str());
        }
        char node[0x20];
        snprintf(node, sizeof node, "select_tile_%zu", picker.built.boxes);
        std::string image = logic::image_of(box);
        add(vm + VM_TILES, alloc, uint8_t(picker.built.boxes), node, box.entrant_list.c_str(), 1, box.title.c_str(), list.c_str(),
            image.c_str(), 0);
        picker.built.lists.push_back(list);
        picker.built.boxes++;
    }
    ref<uint8_t *>(vm, VM_TILES_NODE) = tiles_node;
    if (!renarrow(mode)) {
        put_back_drs();
        if (tt)
            keep_time_trial_lists(mode);
    }
    if (auto *root = at<uint8_t *>(vm, VM_ROOT)) {
        uint8_t visible = picker.built.boxes > 0;
        reinterpret_cast<Vm_Set_Bool_t *>(b.set_bool)(registry(), root + 8, logic::ADDON_TILES_VISIBLE, &visible, 1);
    }
    if (picker.lines.builds.note())
        swofty::log("[era] era select%s: %zu stock box(es), %zu addon box(es) of %zu registered", tt ? " (Time Trial)" : "",
                    stock, picker.built.boxes, order.size());
}

char *press_entry(void *vm, void *command)
{
    auto *cmd = command ? *static_cast<const uint32_t **>(command) : nullptr;
    if (!cmd || !mem::readable(cmd, 4) || *cmd != logic::ADDON_PRESS)
        return picker.press_orig(vm, command);
    auto *v = static_cast<uint8_t *>(vm);
    auto *begin = at<uint8_t *>(v, VM_TILE_BEGIN);
    auto *end = at<uint8_t *>(v, VM_TILE_END);
    size_t count = end > begin ? size_t(end - begin) / TILE_ENTRY : 0;
    std::string why = logic::press_refusal(v == picker.built.vm, picker.set.list.node && picker.set.selected.node, count,
                                           picker.built.stock, picker.built.boxes);
    if (!why.empty()) {
        if (picker.lines.refused.note())
            swofty::log("[era] addon box press refused: %s", why.c_str());
        return nullptr;
    }
    uint8_t *tiles_node = at<uint8_t *>(v, VM_TILES_NODE), *selected_node = at<uint8_t *>(v, VM_SELECTED_NODE);
    ref<uint8_t *>(v, VM_TILES_NODE) = picker.set.list.node;
    ref<uint8_t *>(v, VM_SELECTED_NODE) = picker.set.selected.node;
    ref<uint8_t *>(v, VM_TILE_BEGIN) = begin + picker.built.stock * TILE_ENTRY;
    uint32_t select = logic::SELECT_PRESS;
    const void *fake[4] = {&select, nullptr, nullptr, nullptr};
    char *r = picker.press_orig(vm, fake);
    ref<uint8_t *>(v, VM_TILES_NODE) = tiles_node;
    ref<uint8_t *>(v, VM_SELECTED_NODE) = selected_node;
    ref<uint8_t *>(v, VM_TILE_BEGIN) = begin;
    return r;
}

void select_entry(void *container, const char *entrant_list)
{
    {
        std::lock_guard<std::mutex> g(hold.lock);
        put_back();
    }
    void *mode = container ? at<void *>(container, CONTAINER_MODE) : nullptr;
    bool ours =
        entrant_list && std::find(picker.built.lists.begin(), picker.built.lists.end(), entrant_list) != picker.built.lists.end();
    if (mode && ours)
        narrow(mode, entrant_list);
    else {
        restore();
        clear_fuel();
    }
    if (mode && !ours && time_trial(mode))
        keep_time_trial_lists(mode);
    picker.last_press = entrant_list ? entrant_list : "";
    if (ours && !mode && picker.lines.narrow.note())
        swofty::log("[era] %s pressed but the container has no mode; %s is not narrowed", entrant_list, logic::HOST);
    picker.last_pick = ours ? entrant_list : "";
    swofty_core::apply::moments::box_picked(entrant_list, mode);
    if (auto *modes = setup_owner(mode))
        ref<void *>(modes, SELECTED_LIST) = nullptr;
    std::string selected = ours ? pressed_list(entrant_list) : std::string(entrant_list ? entrant_list : "");
    picker.select_orig(container, entrant_list ? selected.c_str() : entrant_list);
}

}
