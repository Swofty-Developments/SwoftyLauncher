// The hud.slot reader (src/core/hud/slots_logic.hpp) over a fake heap, and the placement math of
// swofty/hud_slot.hpp. Run by native/test/run_hud_slots_test.sh.
#include "core/hud/slots_logic.hpp"

#include <swofty/hud_slot.hpp>

#include <cmath>
#include <cstdio>
#include <cstring>
#include <vector>

namespace s = hud::slots;
namespace hs = swofty::hud_slot;

int failures = 0;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

bool near(float a, float b) { return std::fabs(a - b) < 0.01f; }

struct Heap {
    std::vector<uint8_t> bytes = std::vector<uint8_t>(0x10000);
    static constexpr uintptr_t BASE = 0x10000;
    bool readable(uintptr_t a, size_t n) const { return a >= BASE && a + n <= BASE + bytes.size(); }
    uintptr_t translate(uintptr_t a) const { return reinterpret_cast<uintptr_t>(bytes.data()) + (a - BASE); }
    template <class T> void put(uintptr_t a, T v) { std::memcpy(bytes.data() + (a - BASE), &v, sizeof v); }
};

constexpr uintptr_t DATA = 0x11000, ENTRIES = 0x14000, LISTEN = 0x15000, CTRL = 0x15100, LAYOUTS = 0x15400, LAYOUT = 0x15500,
                    KEYS = 0x15600, BOXES = 0x15800, BOX = 0x15900;

void entry(Heap &h, int i, uint32_t key, float x, float y, bool on, float scale)
{
    uintptr_t at = ENTRIES + uintptr_t(i) * s::ENTRY;
    h.put(at, key);
    h.put(at + 4, key);
    h.put(at + s::E_X, x);
    h.put(at + s::E_Y, y);
    h.put<uint8_t>(at + s::E_ENABLED, on);
    h.put(at + s::E_OPACITY, 1.0f);
    h.put(at + s::E_SCALE, scale);
}

void test_keys()
{
    CHECK(s::djb2("racedial_f1") == 647083312u);
    CHECK(s::djb2("qualifyingminimap") == 1516591561u);
    CHECK(s::key("acme.radio", "Card-1") == "swofty_acme_radio_card_1");
}

void test_reader()
{
    Heap h;
    std::string k = s::key("acme.radio", "card");
    uint32_t race = s::djb2("race" + k), quali = s::djb2("qualifying" + k);
    const int cap = 8;
    h.put(DATA + s::MAP + s::MAP_ENTRIES, ENTRIES);
    uintptr_t info = ENTRIES + cap * s::ENTRY;
    h.put(DATA + s::MAP + s::MAP_INFO, info);
    entry(h, 2, quali, -1.0f, 2.0f, true, 1.5f);
    h.put<uint8_t>(info + 2, 1);
    entry(h, 5, race, 3.0f, -1.0f, false, 0.75f);
    h.put<uint8_t>(info + 5, 1);
    entry(h, 6, 1234, 0, 0, true, 1);
    s::Reader<Heap> r{h};
    s::Entry e = r.entry(DATA, race);
    CHECK(e.found && near(e.x, 3) && near(e.y, -1) && !e.enabled && near(e.scale, 0.75f));
    CHECK(!r.entry(DATA, 1234).found);
    CHECK(!r.entry(DATA, 99).found);

    h.put(DATA + s::LISTENERS, LISTEN);
    h.put(DATA + s::LISTENERS_END, LISTEN + 2 * s::LISTENER);
    h.put(LISTEN + 8, uintptr_t(0x15200));
    h.put(LISTEN + s::LISTENER + 8, CTRL);
    CHECK(r.editor(DATA) == 0);
    h.put<uint8_t>(CTRL + s::C_EDITOR, 1);
    CHECK(r.editor(DATA) == CTRL);
    CHECK(r.any_controller(DATA) == CTRL);

    h.put(DATA + s::LAYOUT_LIST, LAYOUTS);
    h.put(DATA + s::LAYOUT_LIST_END, LAYOUTS + 16);
    h.put(LAYOUTS + 8, LAYOUT);
    h.put<int32_t>(CTRL + s::C_LAYOUT, 1);
    h.put<uint32_t>(LAYOUT + s::L_COUNT, 2);
    h.put(LAYOUT + s::L_KEYS, KEYS);
    h.put<uint32_t>(KEYS, 7);
    h.put<uint32_t>(KEYS + 4, quali);
    CHECK(r.layout_has(DATA, CTRL, quali));
    CHECK(!r.layout_has(DATA, CTRL, race));
    h.put<int32_t>(CTRL + s::C_LAYOUT, 5);
    CHECK(!r.layout_has(DATA, CTRL, quali));

    h.put(CTRL + s::C_BOXES, BOXES);
    h.put(CTRL + s::C_BOXES_INFO, BOXES + 4 * 8);
    h.put(BOXES + 3 * 8, BOX);
    h.put<uint8_t>(BOXES + 4 * 8 + 3, 1);
    h.put(BOX, quali);
    h.put(BOX + s::B_MIN, -2.0f);
    h.put(BOX + s::B_MIN + 4, 1.0f);
    h.put(BOX + s::B_MAX, 1.0f);
    h.put(BOX + s::B_MAX + 4, 3.0f);
    CHECK(!r.box(CTRL, quali).valid);
    h.put<uint8_t>(BOX + s::B_VALID, 1);
    s::Box b = r.box(CTRL, quali);
    CHECK(b.valid && near(b.x0, -2) && near(b.y1, 3));

    h.put(DATA + s::MAP + s::MAP_INFO, ENTRIES + 3);
    CHECK(!r.entry(DATA, race).found);
}

void test_place()
{
    hs::Slot slot{"card", "Card", 0.02f, 0.55f, 0.28f, 0.12f, hs::Anchor::left};
    SwoftyHudSlotState st{};
    st.size = sizeof st;
    hs::Placement p = hs::place(slot, st, 1920, 1080);
    CHECK(near(p.x, 0.02f * 1920) && near(p.y, 0.55f * 1080) && near(p.w, 0.28f * 1920) && near(p.h, 0.12f * 1080));
    CHECK(p.visible && !p.live);
    float ux, uy;
    hs::default_offset(slot, ux, uy);
    st.found = 1;
    st.enabled = 1;
    st.x = ux + 1.0f;
    st.y = uy - 0.5f;
    st.scale = 2;
    st.opacity = 1;
    p = hs::place(slot, st, 1920, 1080);
    float cx = (0.02f + 0.14f) * 1920 + 100, cy = (0.55f + 0.06f) * 1080 + 50;
    CHECK(near(p.w, 0.56f * 1920) && near(p.h, 0.24f * 1080));
    CHECK(near(p.x + p.w / 2, cx) && near(p.y + p.h / 2, cy));
    p = hs::place(slot, st, 960, 540);
    CHECK(near(p.x + p.w / 2, cx / 2));
    st.has_box = 1;
    st.box[0] = -9.6f;
    st.box[1] = 4.4f;
    st.box[2] = -8.6f;
    st.box[3] = 5.4f;
    p = hs::place(slot, st, 1920, 1080);
    CHECK(near(p.x, 0) && near(p.y, 0) && near(p.w, 100) && near(p.h, 100));
    st.enabled = 0;
    CHECK(!hs::place(slot, st, 1920, 1080).visible);
}

int main()
{
    test_keys();
    test_reader();
    test_place();
    if (failures) {
        std::printf("hud_slots_test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("hud_slots_test: ok\n");
    return 0;
}
