#pragma once
#include <safetyhook.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

#include "core/frontend/game_message_log_logic.hpp"

namespace frontend::popup {

struct FixedId {
    const void *vt;
    uint64_t cap;
    char *data;
    char buf[128];
};

struct Handle {
    void *sys;
    void *msg;
};

struct Callback {
    void *fn;
    void *ctx;
};

using Construct_t = Handle *(void *sys, Handle *out, FixedId *id, Callback *cb, const char *a5, uint64_t a6);
using SetAttr_t = bool(Handle *h, const char *name, void **value, uint32_t flags, const char *a5);
using Release_t = void(Handle *h);
using MakeStr_t = uint32_t(void **out, const char *s, uint32_t flags, uintptr_t len);

struct Request {
    std::string title, body, by;
    bool confirm = false;
    Callback cb{};
    void (*failed)(void *ctx) = nullptr;
};

struct Toast {
    std::string text, detail, icon, by;
    unsigned seconds;
};

constexpr size_t MAX_TOASTS = 8, MAX_TOAST_TEXT = 200;

int api_available();
std::string who(const void *from);
int api_toast(const char *text, const char *detail, float seconds, const char *icon);

struct MessageSystem {
    Handle *constructed(void *s, Handle *out, FixedId *id, Callback *cb, const char *a5, uint64_t a6);
    int enqueue(std::string t, std::string b, const std::string &by);
    int enqueue_confirm(std::string t, std::string b, void *fn, void *ctx, void (*failed)(void *), const std::string &by);
    int toast_enqueue(std::string text, std::string detail, float seconds, std::string icon, const std::string &by);
    void setup();
    void toast_test_tick();
    void tick();

    void log_game_message(const char *name, bool built);
    bool make_id(FixedId &id, const char *name);
    bool set(Handle &h, const char *name, const std::string &text);
    Handle open(void *s, const char *name, Callback *cb = nullptr);
    void show(void *s, const Request &r);
    void show_toast(void *s, const Toast &t);

    uintptr_t a_construct = 0, a_set_attr = 0, a_release = 0, a_make_str = 0;
    const void *id_vt = nullptr;
    safetyhook::InlineHook h_construct;
    std::atomic<void *> sys{nullptr};
    std::atomic<bool> live{false}, own_missing{false};
    std::mutex queue_lock;
    std::deque<Request> queue;
    std::atomic<size_t> pending{0}, toasts_pending{0};
    std::deque<Toast> toasts;
    std::atomic<bool> toast_missing{false};
    std::mutex game_messages_lock;
    frontend::game_message_log::Seen game_messages;
    unsigned toast_test_every = 0;
    uint64_t toast_test_next = 0;
    unsigned toast_test_count = 0;
};

extern MessageSystem messages;

}
