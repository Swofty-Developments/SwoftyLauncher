#include "core/career/content_state.hpp"
#include "core/guards/skips.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstring>
#include <mutex>
#include <string>

// The career contents' tiles (career_content.cpp registers the contents and places the hooks).
//
// Tile (site_FeTile_CareerContentEnd, the jmp that ends FeTile_Build_List case 6, r14 = the
// TileList): the showroom case (menu kind 4) ends through the same jmp, so the site does nothing unless
// the list's view model is the content select's or the list holds a stock career tile.
// FeHome_Add_CareerTile appends button_N (template, VM node, list entry); its 0x148 home item is
// replaced, the way case 6 replaces items, by a career content item
// (FeTile_Construct_CareerItem) with the model tile's route (+0x178), the content's data group
// (+0x180) and suffix (+0x188), and a copy of the model item's vtable whose populate (slot 4) shows
// the content's title and description and whose press (slot 5) selects its car group
// (g_FeTile_SelectedCareerContent), takes the model tile's menu systems (+0x150..+0x178) when its
// own are unset, and calls the router FeTile_Route_CareerItem.
//
// Addons row: the content select screen (game::FECareerDLCSelectViewModel, its TileList at +0x2c8)
// gets a second spawner, addon_buttons, from the UI build (packages/core/src/games/f1_25/ui/career_select.rs).
// The site finds the view model on the stack, takes the addon_buttons list node under its root and
// appends each content tile while the TileList's buttons node points at it, so the tile's VM node
// lands in the Addons row and its entry in the same item vector after the stock tiles. The row raises
// addon_select_pressed and addon_focus_changed; the view model's command handler
// (vt_Game_CareerDLCSelectViewModel slot 10) runs them as the stock select_pressed and focus_changed
// while the buttons node and the item vector start at the row, so the stock selected-item lookup reads
// the pressed content tile. A content shows in Driver Career, My Team or both (desc.careers); the
// career is the last content select pick route (career_start.cpp).
// A spawned tile goes to the scene element its template's layout names, and the stock tile template
// (fe_tile_button_default) names spawner_stacker_items, so the Addons row's tiles get the type
// addon_tile_button_default, the UI build's copy laid out in addon_stacker_items. They are numbered
// after the stock tiles (button_3, ...).

namespace career::content {

namespace {

using AddTile = void(TileList *, uint8_t);
using Construct = HomeItem *(HomeItem *, NeAllocator *, const char *);
using Populate = void(HomeItem *, void *, uint32_t);
using MakeStr = uint32_t(NeStr **, const char *, uint32_t, uintptr_t);
using SetString = void *(void *, void *, const char *, NeStr **, int);

// The last content select build: its tile list and view model, its Addons row list node, and how many of
// the list's items are stock tiles and content tiles.
struct Row {
    TileList *list = nullptr;
    void *vm = nullptr;
    uint8_t *addon = nullptr;
    size_t stock = 0, tiles = 0;
    uint32_t career = content::CAREER_DRIVER;
};

std::atomic<TileList *> last_list{nullptr};
std::mutex row_lock;
Row row;
constinit swofty_core::guards::LogFirst<LOG_FIRST> row_lines, rowless_lines, refused_lines, other_lines;

void set_field(void *field, const char *text)
{
    reinterpret_cast<MakeStr *>(b.make_str)(static_cast<NeStr **>(field), text, 0, 0);
}

void set_text(void *node, const char *name, const char *text)
{
    NeStr *s = nullptr;
    set_field(&s, text);
    reinterpret_cast<SetString *>(b.set_string)(*reinterpret_cast<void **>(b.vm_registry), node, name, &s, 1);
    set_field(&s, nullptr);
}

uintptr_t model_populate(uint32_t model) { return model == content::MODEL_2025 ? b.populate_2025 : b.populate_2026; }

bool is_model(const HomeItem *item, uint32_t model)
{
    void *const *vt = item && mem::readable(item, 8) ? *reinterpret_cast<void *const *const *>(item) : nullptr;
    return vt && mem::readable(vt + content::ITEM_POPULATE_SLOT, 8) &&
           reinterpret_cast<uintptr_t>(vt[content::ITEM_POPULATE_SLOT]) == model_populate(model);
}

HomeItem *find_in(TileList *list, uint32_t model, const HomeItem *self, bool *has_self)
{
    HomeItem *found = nullptr;
    if (has_self)
        *has_self = false;
    if (!list || !list->items_begin || list->items_end <= list->items_begin)
        return nullptr;
    for (auto *e = list->items_begin; e < list->items_end; e++) {
        HomeItem *item = e->tile ? e->tile->item : nullptr;
        if (has_self && item == self)
            *has_self = true;
        if (!found && is_model(item, model))
            found = item;
    }
    return found;
}

void populate(HomeItem *item, void *node, uint32_t a7)
{
    Slot *s = by_vtable(item);
    uint32_t model = s ? s->desc.model : content::MODEL_2026;
    reinterpret_cast<Populate *>(model_populate(model))(item, node, a7);
    auto *vm = static_cast<uint8_t *>(item->button_node.node);
    if (!s || !vm)
        return;
    if (s->in_row)
        set_text(vm + 8, "type", content::ADDON_TEMPLATE);
    set_text(vm + 8, "title", s->desc.title.c_str());
    set_text(vm + 8, "description", s->desc.description.c_str());
    if (model == content::MODEL_2026 || s->desc.badge) {
        auto **vt = *reinterpret_cast<void ***>(item);
        reinterpret_cast<void (*)(HomeItem *, int)>(vt[content::ITEM_NOTIFICATION_SLOT])(item, int(s->desc.badge));
    }
}

bool adopt_systems(HomeItem *item, const Slot &s)
{
    auto *raw = reinterpret_cast<uint8_t *>(item);
    if (content::at<void *>(raw, content::ITEM_SYSTEM_PROBE))
        return true;
    bool mine = false;
    HomeItem *model = find_in(last_list.load(), s.desc.model, item, &mine);
    if (!mine || !model || !content::at<void *>(model, content::ITEM_SYSTEM_PROBE))
        return false;
    std::memcpy(raw + content::ITEM_SYSTEMS, reinterpret_cast<const uint8_t *>(model) + content::ITEM_SYSTEMS,
                content::ITEM_SYSTEMS_SIZE);
    swofty::log("[career] content '%s': its tile took the %s tile's menu systems", s.desc.id.c_str(),
                s.desc.model == content::MODEL_2025 ? "2025" : "2026");
    return true;
}

Action *press(HomeItem *item, Action *out)
{
    Slot *s = by_vtable(item);
    const uint8_t *eb = nullptr, *ee = nullptr;
    car_entries(b.customisation, eb, ee);
    content::Pick pick = s ? content::pick(eb, ee, s->desc.car_group, s->desc.model) : content::Pick{};
    *reinterpret_cast<uint32_t *>(b.selected) = pick.id;
    if (s && s->presses++ < LOG_FIRST)
        swofty::log("[career] content '%s' pressed: car group 0x%08x (%s)", s->desc.id.c_str(), pick.id,
                    pick.kind == content::Pick::exact      ? "its car_group"
                    : pick.kind == content::Pick::fallback ? "the model tile's pick"
                                                           : "no content entry");
    if (!s || !adopt_systems(item, *s)) {
        out->type = 1;
        out->str = nullptr;
        swofty::log("[career] content '%s': the tile has no menu systems and the model tile has none to copy; press "
                    "ignored",
                    s ? s->desc.id.c_str() : "?");
        return out;
    }
    return reinterpret_cast<Route *>(b.route)(item, out);
}

HomeItem *build_item(NeAllocator *alloc, HomeItem *model, Slot &s)
{
    bool y25 = s.desc.model == content::MODEL_2025;
    auto alloc_fn = reinterpret_cast<void *(*)(NeAllocator *, size_t, size_t)>(alloc->vtable->alloc);
    auto *item = static_cast<HomeItem *>(alloc_fn(alloc, y25 ? content::ITEM_SIZE_2025 : content::ITEM_SIZE_2026, 0x10));
    if (!item)
        return nullptr;
    const char *id = nestr(model->id);
    reinterpret_cast<Construct *>(b.construct)(item, alloc, id ? id : (y25 ? "career_default_content" : "career_dlc_content"));
    auto *raw = reinterpret_cast<uint8_t *>(item);
    const char *route = nestr(content::at<const NeStr *>(model, content::ITEM_ROUTE));
    set_field(raw + content::ITEM_ROUTE, route && *route ? route : "driver_career");
    if (!y25) {
        const char *ent = nestr(content::at<const NeStr *>(model, content::ITEM_ENTITLEMENT_NAME));
        std::memset(raw + content::ITEM_ENTITLEMENT, 0, 0x10);
        set_field(raw + content::ITEM_ENTITLEMENT_NAME, !s.desc.entitlement.empty() ? s.desc.entitlement.c_str()
                                                        : ent && *ent               ? ent
                                                                                    : "f1_2026");
    }
    set_field(raw + content::ITEM_DATA_GROUP, s.desc.data_group.c_str());
    set_field(raw + content::ITEM_SUFFIX, s.desc.suffix.c_str());
    raw[content::ITEM_IS_DLC] = 0;
    if (!s.vt) {
        auto **src = *reinterpret_cast<void ***>(model);
        auto **copy = new void *[content::ITEM_VTABLE_SLOTS + 1];
        std::memcpy(copy, src - 1, (content::ITEM_VTABLE_SLOTS + 1) * sizeof(void *));
        copy[1 + content::ITEM_POPULATE_SLOT] = reinterpret_cast<void *>(&populate);
        copy[1 + content::ITEM_PRESS_SLOT] = reinterpret_cast<void *>(&press);
        s.vt = copy + 1;
    }
    *reinterpret_cast<void ***>(item) = s.vt;
    return item;
}

void add_tile(TileList *list, Slot &s, uint8_t index, bool in_row)
{
    HomeItem *model = find_in(list, s.desc.model, nullptr, nullptr);
    if (!model) {
        turn_off(s, std::string("the content list has no ") + (s.desc.model == content::MODEL_2025 ? "2025" : "2026") +
                        " tile to model the tile on");
        return;
    }
    size_t before = size_t(list->items_end - list->items_begin);
    reinterpret_cast<AddTile *>(b.add_tile)(list, index);
    size_t after = list->items_begin ? size_t(list->items_end - list->items_begin) : 0;
    Tile *tile = after == before + 1 ? list->items_end[-1].tile : nullptr;
    auto *tile_alloc = tile ? content::at<NeAllocator *>(tile, content::TILE_ALLOCATOR) : nullptr;
    if (!tile || !tile->item || !tile_alloc) {
        swofty::log("[career] content '%s': the tile adder gave %zu -> %zu items; no tile", s.desc.id.c_str(), before, after);
        return;
    }
    s.in_row = in_row;
    if (auto *node = static_cast<uint8_t *>(tile->item->button_node.node); in_row && node)
        set_text(node + 8, "type", content::ADDON_TEMPLATE);
    auto *alloc = content::at<NeAllocator *>(list, content::LIST_ALLOCATOR);
    HomeItem *item = alloc ? build_item(alloc, model, s) : nullptr;
    if (!item) {
        swofty::log("[career] content '%s': could not build its content item; button_%u stays a Career tile", s.desc.id.c_str(),
                    unsigned(index));
        return;
    }
    HomeItem *old = tile->item;
    tile->item = item;
    std::memcpy(reinterpret_cast<uint8_t *>(tile) + content::TILE_ALLOCATOR, &alloc, sizeof alloc);
    reinterpret_cast<void (*)(HomeItem *, int)>(reinterpret_cast<void *const *>(old->vtable)[0])(old, 0);
    reinterpret_cast<void (*)(NeAllocator *, void *)>(tile_alloc->vtable->free)(tile_alloc, old);
    if (s.tiles++ < LOG_FIRST)
        swofty::log("[career] content '%s': tile added as button_%u (items %zu -> %zu)", s.desc.id.c_str(), unsigned(index),
                    before, after);
}

void *registry() { return *reinterpret_cast<void **>(b.vm_registry); }

void *buttons_of(const TileList *list) { return content::at<void *>(list, content::LIST_BUTTONS); }

void set_buttons(TileList *list, void *node)
{
    std::memcpy(reinterpret_cast<uint8_t *>(list) + content::LIST_BUTTONS, &node, sizeof node);
}

uint8_t *list_node(uint8_t *root, uint32_t hash)
{
    auto **h = reinterpret_cast<swofty::game::Vm_GetOrCreate_ListChild_t *>(b.list_child)(registry(), root + 8, &hash);
    return h ? static_cast<uint8_t *>(*h) : nullptr;
}

// The view model that owns the tile list: the stock tile builder runs under its command or screen code,
// so a frame on the stack holds a pointer to it, and its tile list pointer names this list. The scan
// stops at the first unreadable page: the stack can end within STACK_SCAN of the builder's frame.
void *owner_of(TileList *list, const SwoftyContext *c)
{
    auto *sp = reinterpret_cast<const uint8_t *>(c->rsp);
    if (!sp)
        return nullptr;
    for (size_t off = 0; off < content::STACK_SCAN; off += 8) {
        if ((off == 0 || (reinterpret_cast<uintptr_t>(sp + off) & 0xfff) == 0) && !mem::readable(sp + off, 8))
            return nullptr;
        auto *v = content::at<uint8_t *>(sp, off);
        if (v && mem::readable(v, content::VM_TILE_LIST + 8) && content::at<TileList *>(v, content::VM_TILE_LIST) == list)
            return v;
    }
    return nullptr;
}

// The Addons row's list node under the view model's root, once the root is the parent of this list's
// buttons node.
uint8_t *addon_row(void *vm, TileList *list, uint8_t **root_out)
{
    auto *root = vm ? content::at<uint8_t *>(vm, content::VM_ROOT) : nullptr;
    if (!root || !b.list_child || !mem::readable(root, 0x10))
        return nullptr;
    if (list_node(root, content::BUTTONS) != static_cast<uint8_t *>(buttons_of(list)))
        return nullptr;
    *root_out = root;
    return list_node(root, content::ADDON_BUTTONS_HASH);
}

bool content_select(TileList *list, void *vm, uintptr_t &vt)
{
    vt = vm && mem::readable(vm, 8) ? content::at<uintptr_t>(vm, 0) : 0;
    const char *ids[content::SELECT_SCAN_TILES] = {};
    size_t n = 0;
    if (!vt || vt != b.select_vt)
        for (auto *e = list->items_begin; e && e < list->items_end && n < content::SELECT_SCAN_TILES; e++) {
            HomeItem *item = e->tile ? e->tile->item : nullptr;
            ids[n++] = item && mem::readable(item, sizeof(HomeItem)) ? nestr(item->id) : nullptr;
        }
    return content::content_select(vt, b.select_vt, ids, n);
}

}

void site_entry(SwoftyContext *c)
{
    auto *list = reinterpret_cast<TileList *>(c->r14);
    if (!list || !mem::readable(list, sizeof(TileList)))
        return;
    void *vm = owner_of(list, c);
    uintptr_t vt = 0;
    if (!content_select(list, vm, vt)) {
        if (other_lines.note())
            swofty::log("[career] tile list of another screen (view model %p, vtable 0x%llx, %zu tile(s)) left as is", vm,
                        (unsigned long long)vt, list->items_begin ? size_t(list->items_end - list->items_begin) : size_t(0));
        return;
    }
    resolve_all(true, true);
    shown_from_records();
    last_list = list;
    uint32_t career = content::career_of(runtime::career_start_picked());
    uint8_t *root = nullptr;
    uint8_t *addon = addon_row(vm, list, &root);
    size_t stock = list->items_begin ? size_t(list->items_end - list->items_begin) : 0;
    void *buttons = buttons_of(list);
    size_t tiles = 0;
    for (size_t i = 0; i < used(); i++) {
        Slot &s = slots[i];
        if (!ready(s) || !(s.desc.careers & career))
            continue;
        if (addon)
            set_buttons(list, addon);
        add_tile(list, s, uint8_t(stock + tiles + 1), addon != nullptr);
        set_buttons(list, buttons);
        tiles++;
    }
    if (root && b.set_bool) {
        uint8_t visible = tiles > 0;
        reinterpret_cast<swofty::game::Vm_Set_Bool_t *>(b.set_bool)(registry(), root + 8, content::ADDON_BUTTONS_VISIBLE,
                                                                    &visible, 1);
    }
    {
        std::lock_guard<std::mutex> g(row_lock);
        row = addon ? Row{list, vm, addon, stock, tiles, career} : Row{};
    }
    if (addon && row_lines.note())
        swofty::log("[career] %s content select: %zu stock tile(s), %zu content tile(s) in the Addons row",
                    content::career_name(career), stock, tiles);
    if (!addon && tiles && rowless_lines.note())
        swofty::log("[career] %s content select: no Addons row (%s); %zu content tile(s) follow the stock tiles",
                    content::career_name(career),
                    !b.list_child ? "Vm_GetOrCreate_ListChild does not resolve"
                    : !vm         ? "its view model was not found on the stack"
                                  : "the view model root is not the buttons list's parent",
                    tiles);
}

// vt_Game_CareerDLCSelectViewModel slot 10. An Addons row command runs as the stock select_pressed or
// focus_changed while the tile list's buttons node and item vector start at the row, so the stock
// selected-item lookup reads the row; every other command goes to the game unchanged.
void command_entry(void *vm, void *cmd, void *data)
{
    auto *name = cmd ? *static_cast<const uint32_t *const *>(cmd) : nullptr;
    uint32_t stock_hash = name && mem::readable(name, 4) ? content::stock_command(*name) : 0;
    if (!stock_hash)
        return hooks.select_command(vm, cmd, data);
    Row r;
    {
        std::lock_guard<std::mutex> g(row_lock);
        r = row;
    }
    auto *list = vm && mem::readable(static_cast<uint8_t *>(vm) + content::VM_TILE_LIST, 8)
                     ? content::at<TileList *>(vm, content::VM_TILE_LIST)
                     : nullptr;
    size_t items = list && list->items_begin ? size_t(list->items_end - list->items_begin) : 0;
    if (!list || list != r.list || vm != r.vm || !r.tiles || items != r.stock + r.tiles) {
        if (refused_lines.note())
            swofty::log("[career] Addons row command 0x%08x ignored: the content select was rebuilt or is not the one "
                        "the row was built for (%zu items, row built with %zu + %zu)",
                        *name, items, r.stock, r.tiles);
        return;
    }
    struct {
        uint32_t hash = 0;
        uint16_t refs = 0x7fff;
        uint8_t len[3] = {};
        char chars[24] = {};
    } fake;
    fake.hash = stock_hash;
    const char *text = stock_hash == content::SELECT_PRESSED ? "select_pressed" : "focus_changed";
    std::memcpy(fake.chars, text, std::strlen(text));
    fake.len[0] = uint8_t(std::strlen(text));
    void *fake_cmd = &fake;
    void *buttons = buttons_of(list);
    auto *begin = list->items_begin;
    set_buttons(list, r.addon);
    list->items_begin = begin + r.stock;
    hooks.select_command(vm, &fake_cmd, data);
    set_buttons(list, buttons);
    list->items_begin = begin;
}

}
