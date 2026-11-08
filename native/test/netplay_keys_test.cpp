// Host unit test for the session key material answer (netplay_keys_logic.hpp). run_events_test.sh builds and
// runs it.
#include "../src/core/netplay_keys_logic.hpp"

#include <cstdio>
#include <cstring>
#include <string>

namespace keys = runtime::netplay_keys;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static int calls;
static bool seen_success;
static void *seen_closure;
static std::string seen_parts[3];
static bool seen_each_capacity_fits;

static void fake_update(void *closure, const bool *success, const void *material)
{
    ++calls;
    seen_closure = closure;
    seen_success = *success;
    auto m = static_cast<const keys::Material *>(material);
    seen_each_capacity_fits = true;
    for (int i = 0; i < 3; ++i) {
        seen_parts[i] = m->part[i].chars ? m->part[i].chars : "";
        seen_each_capacity_fits = seen_each_capacity_fits && m->part[i].capacity > seen_parts[i].size();
    }
}

static void other_slot() {}

struct Callback {
    void *slot[16];
};

static Callback make(const void *const *vtable)
{
    Callback cb{};
    cb.slot[0] = const_cast<void *>(static_cast<const void *>(vtable));
    cb.slot[7] = &cb;
    return cb;
}

int main()
{
    static const void *const key_vtable[] = {nullptr, nullptr, reinterpret_cast<const void *>(&fake_update)};
    static const void *const other_vtable[] = {nullptr, nullptr, reinterpret_cast<const void *>(&other_slot)};
    const void *update = reinterpret_cast<const void *>(&fake_update);

    keys::Material m;
    keys::fill(m);
    for (int i = 0; i < 3; ++i) {
        expect(std::strcmp(m.part[i].chars, "x") == 0, "part " + std::to_string(i) + " is non-empty");
        expect(m.part[i].capacity >= 2, "part " + std::to_string(i) + " capacity holds the text and its terminator");
    }

    Callback cb = make(key_vtable);
    Callback *from_stack = &cb;
    cb.slot[7] = from_stack;
    expect(keys::is_key_callback(cb.slot, update), "the key material closure is recognised");
    expect(keys::answer(cb.slot, update), "the closure is answered");
    expect(calls == 1, "the update ran once");
    expect(seen_success, "the update saw success");
    expect(seen_closure == cb.slot, "the update got the closure object");
    expect(seen_parts[0] == "x" && seen_parts[1] == "x" && seen_parts[2] == "x", "the update saw three non-empty strings");
    expect(seen_each_capacity_fits, "each capacity exceeds its text");
    expect(cb.slot[7] == nullptr, "the callback is marked consumed like the original submit does");

    Callback other = make(other_vtable);
    expect(!keys::is_key_callback(other.slot, update), "another callback is passed through");
    expect(!keys::answer(other.slot, update), "another callback is not answered");
    expect(calls == 1, "no update for another callback");

    Callback heap = make(key_vtable);
    heap.slot[7] = &other;
    expect(!keys::is_key_callback(heap.slot, update), "a callback holding its callable elsewhere is passed through");

    Callback none = make(nullptr);
    expect(!keys::is_key_callback(none.slot, update), "a callback with no vtable is passed through");
    expect(!keys::is_key_callback(nullptr, update), "a null callback is passed through");
    expect(!keys::is_key_callback(cb.slot, nullptr), "an unresolved update never matches");

    printf("%s\n", fails ? "FAILED" : "all passed");
    return fails != 0;
}
