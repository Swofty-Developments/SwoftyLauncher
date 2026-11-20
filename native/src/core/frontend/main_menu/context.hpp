#pragma once
#include "core/frontend/main_menu/navigation.hpp"
#include "core/frontend/main_menu/panel.hpp"
#include "core/frontend/main_menu/tiles.hpp"
#include "core/frontend/main_menu_logic.hpp"

#include <swofty/game.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <mutex>

namespace frontend::main_menu {

constexpr size_t NESTR_CHARS = 9;

struct MainMenu {
    std::atomic<bool> patched{false}, home_open{false}, customisation_armed{false}, tt_warned{false};
    std::atomic<int> via{0};
    std::atomic<uint64_t> home_opened_at{0};
    std::mutex view_lock;
    HomeView home_view;
    Tiles tiles;
    Navigation navigation;
    Panel panel;
};

extern MainMenu menu;

const char *env(const char *name);
bool off(const char *name);
const char *id_of(const swofty::game::HomeItem *item);

}
