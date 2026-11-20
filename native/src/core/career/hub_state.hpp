#pragma once

#include <safetyhook.hpp>
#include <swofty/mem.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

// What the hub tabs' setup and laptop builder (hub_tab.cpp), the Driver Career Team HQ builder (hub_team_hq.cpp) and
// the R&D tab (hub_rnd.cpp) share: the tabs read from settings.json, the game functions setup() resolves for the
// builders, the page both builders add, hide_rnd and its pending change, which builders follow it live, the R&D
// callbacks of the builder hooks setup() places, and the hub's capped note line.
namespace career::hub {

struct HubOption {
    std::string id, label, description;
};

struct HubTab {
    std::string addon, name, title, group, lng, content, data;
    std::vector<HubOption> options;
    uint32_t key = 0;
};

constexpr size_t TAB_ALIGN = 0x10, PAGE_SIZE = 0x660;
constexpr uint32_t PAGE_TYPE = 5;

using TabCtor_t = void(void *tab, void *a2, void *a3, void *laptop, const char *content, const char *header, const char *name,
                       const char *icon, uint8_t flag);
using Push_t = void(void *tabs, const uint32_t *key, void *owned);
using PageCtor_t = void(void *page, void *a2, void *a3, void *owner, const char *content, const char *name, void *a7,
                        const char *a8, uint32_t page_type, uint8_t flag);

extern std::vector<HubTab> tabs;
extern uintptr_t a_push, a_page_ctor;
extern uintptr_t a_tq_ctor, a_tq_vt0, a_tq_vt260, a_tq_vt268, a_tq_init, a_tq_push;

// Publishes a tab's page data at its view_model_data path.
void publish(const HubTab &t);
// Gives a page the copied vtable whose onCommandRaised steps the tab's options; false when its vtable is not the
// first page's.
bool own_vtable(void *page);
// The allocator at holder, or nullptr when it is not a game object with an allocate slot in the image.
void *alloc_through(uintptr_t holder);
void *allocate(void *allocator, size_t size, size_t align);

// The Team HQ builder's end hook (hub_team_hq.cpp): adds the active group's tabs after STANDINGS.
void team_hq_end(safetyhook::Context &c);

extern std::atomic<bool> hide, pending, laptop_flag, team_hq_flag;
extern uintptr_t a_set_visible, a_tq_vehicle;

// Logs fmt with up to three strings while fewer than 48 hub notes have been logged.
void note(const char *fmt, const char *a = "", const char *b = "", const char *c = "");

template <class T> T at(uintptr_t p)
{
    T v{};
    swofty::mem::read(p, v);
    return v;
}

void rnd_gate(safetyhook::Context &c);
void team_hq_rnd(safetyhook::Context &c);
void laptop_registered(safetyhook::Context &c);
void team_hq_registered(safetyhook::Context &c);
void team_hq_show_all(safetyhook::Context &c);
int api_hide_rnd(int on);

}
