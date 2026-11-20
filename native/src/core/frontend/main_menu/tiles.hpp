#pragma once
#include <swofty/game.hpp>

#include <atomic>
#include <cstdint>
#include <map>
#include <mutex>

namespace frontend::main_menu {

using Add_t = void (*)(swofty::game::TileList *, uint8_t);
using Populate_t = void (*)(swofty::game::HomeItem *, void *, uint32_t);
using GetAction_t = swofty::game::Action *(*)(swofty::game::HomeItem *, swofty::game::Action *);

class Tiles {
public:
    bool resolve();
    bool skip_stock_block();
    uint8_t add(swofty::game::TileList *list, uint8_t career_index, const uint8_t *flags);
    bool rows_on();
    void style_item(swofty::game::HomeItem *item);

    void populate_typed(swofty::game::HomeItem *item, void *node, uint32_t a7);
    void populate_showroom(swofty::game::HomeItem *item, void *node, uint32_t a7);
    void populate_customisation(swofty::game::HomeItem *item, void *node, uint32_t a7);
    void populate_split_screen(swofty::game::HomeItem *item, void *node, uint32_t a7);
    void populate_card(swofty::game::HomeItem *item, void *node, uint32_t a7);

private:
    struct Block {
        uint8_t *at = nullptr;
        Add_t f1world = nullptr, challenge = nullptr, divider = nullptr, local_mp = nullptr, theatre = nullptr;
    };

    bool menu_on();
    bool customisation_on();
    void wrap_item(swofty::game::HomeItem *item);
    void style_list(swofty::game::TileList *list);
    void retitle(swofty::game::HomeItem *item, const char *title, const char *description);
    void add_theatre_copy(swofty::game::TileList *list, uint8_t index, const char *id, Populate_t populate, GetAction_t action);
    void add_split_screen(swofty::game::TileList *list, uint8_t index);
    void add_card(swofty::game::TileList *list, uint8_t index, const char *id);

    Block block;
    std::atomic<bool> resolved{false};
    std::once_flag once;
    void *f1w_populate = nullptr, *f1w_action = nullptr;
    std::atomic<int> rows_flag{-1}, menu_flag{-1}, customisation_flag{-1};
    void *theatre_populate = nullptr, *theatre_action = nullptr, *local_mp_populate = nullptr;
    std::map<void **, void *> typed_orig;
    std::mutex typed_lock;
};

}
