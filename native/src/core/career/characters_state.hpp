#pragma once

#include <atomic>
#include <cstdint>
#include <mutex>
#include <set>
#include <string>

#include "core/runtime/loader/hook_set.hpp"

// What the character overrides' entries and API (characters.cpp) and their game hooks (characters_hooks.cpp) share.
namespace career::characters {

using BuildRef = void *(void *out, const uint8_t *character, void *node);
using ListLoaded = int64_t(void *ctx, const void *message);
using PostLine = void(void *speech, void *line, uint8_t retrigger);

extern BuildRef *orig_build;
extern ListLoaded *orig_loaded;
extern PostLine *orig_post;
extern std::mutex lock;
extern std::string active;
extern std::set<void *> muted_emitters;
extern std::atomic<bool> any_swap, any_mute;
extern runtime::loader::MixinPart ref;

// The active group's replacement for a character and its muted speakers (characters.cpp).
std::string replacement_for(const std::string &character);
bool speaker_muted(const char *speaker);

// The ref and voice wraps (characters_hooks.cpp), and the character list's rebuild for a new active group.
void *build_ref(void *out, const uint8_t *character, void *node);
int64_t list_loaded(void *ctx, const void *message);
void post_line(void *speech, void *line, uint8_t retrigger);
bool rebuild(const char *why);

}
