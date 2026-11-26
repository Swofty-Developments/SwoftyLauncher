#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <memory>
#include <string>

#include "core/frontend/mfd_pages_logic.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/hooks/game_point.hpp"
#include "core/hooks/hooks.hpp"
#include "core/hooks/mfd_pages_point.hpp"

namespace swofty_core::hooks {
namespace {

namespace mfd = frontend::mfd_pages;
namespace ev = swofty::ui::events;

Table *g_table;
std::unique_ptr<GamePoint> g_point;

struct State {
    void *vm = nullptr;
    uintptr_t epoch = 0;
    uint32_t mask = 0;
    int told = 0;
};

State g_state;

void log_outcome(const mfd::Outcome &o, int count)
{
    if (o.resized)
        swofty::log("[hooks] %s: a handler changed the page count to %d; the list stays at %d pages (only hiding is supported)",
                    mfd_point::POINT, count, mfd::STOCK_COUNT);
    if (o.reordered)
        swofty::log("[hooks] %s: a handler reordered the pages; reordering is refused (the order of the game's page vector is "
                    "not proven), only the hide flags are read, by page name",
                    mfd_point::POINT);
    if (o.unknown)
        swofty::log("[hooks] %s: %d page entr%s with an unknown name ignored", mfd_point::POINT, o.unknown,
                    o.unknown == 1 ? "y" : "ies");
}

uint32_t ask(void *vm)
{
    ev::MfdPage pages[mfd::CAPACITY];
    mfd::fill(pages);
    ev::MfdPages e{};
    e.size = sizeof e;
    e.payload_version = ev::MfdPages::version;
    e.pages = pages;
    e.count = mfd::STOCK_COUNT;
    e.capacity = mfd::CAPACITY;
    int replaced = 0;
    g_table->fire("", ev::MfdPages::point, &e, &replaced);
    mfd::Outcome o = mfd::read(pages, e.count);
    log_outcome(o, e.count);
    std::string hidden;
    for (int i = 0; i < mfd::STOCK_COUNT; i++)
        if (o.mask & (1u << i))
            hidden += (hidden.empty() ? "" : ", ") + std::string(mfd::STOCK[i]);
    swofty::log("[hooks] %s: MFD vm %p built; hidden by handlers: %s", mfd_point::POINT, vm,
                hidden.empty() ? "none" : hidden.c_str());
    return o.mask;
}

void d_refresh(void *vm)
{
    g_point->original<void>(vm);
    uintptr_t epoch = frontend::ui_vm_epoch();
    if (vm != g_state.vm || epoch != g_state.epoch) {
        g_state.vm = vm;
        g_state.epoch = epoch;
        g_state.mask = ask(vm);
    }
    for (int i = 0; i < mfd::STOCK_COUNT; i++) {
        if (!(g_state.mask & (1u << i)))
            continue;
        int32_t off = 0;
        if (!frontend::ui_vm_set_path(mfd::enabled_path(mfd::STOCK[i]), 1, &off) && g_state.told++ < 4)
            swofty::log("[hooks] %s: could not write %s_enabled; the page stays as the game left it", mfd_point::POINT,
                        mfd::STOCK[i]);
    }
}

}

void declare_mfd_pages_point(Table &t)
{
    g_table = &t;
    g_point = std::make_unique<GamePoint>(mfd_point::POINT, "Hud_MfdVm_RefreshPanelAvailability",
                                          reinterpret_cast<void *>(&d_refresh), game_ops());
    t.declare_game(mfd_point::POINT, SWOFTY_POINT_MODIFY, ev::MfdPages::version, g_point.get());
}

}
