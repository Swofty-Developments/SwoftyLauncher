#pragma once

#include <safetyhook.hpp>
#include <swofty/mem.hpp>

#include <atomic>
#include <cstdint>

// What the Team HQ hub tabs' builders (hub_tab.cpp) and their R&D tab (hub_rnd.cpp) share: hide_rnd and its
// pending change, which builders follow it live, the two addresses setup() resolves for the R&D code, the R&D
// callbacks of the builder hooks setup() places, and the hub's capped note line.
namespace career::hub {

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
