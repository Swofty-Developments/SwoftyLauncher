#include "entitlements_logic.hpp"

#include <cstdio>
#include <cstdlib>
#include <map>

namespace ent = runtime::entitlements;

#define CHECK(c)                                                                  \
    do {                                                                          \
        if (!(c)) {                                                               \
            std::fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #c); \
            std::exit(1);                                                         \
        }                                                                         \
    } while (0)

struct Mem {
    std::map<uint64_t, uint64_t> words;
    bool u64(uint64_t a, uint64_t &out)
    {
        auto it = words.find(a);
        if (it == words.end())
            return false;
        out = it->second;
        return true;
    }
};

int main()
{
    CHECK(ent::tracked("f1_2026") && !ent::tracked("f1_2025") && !ent::tracked(nullptr) && !ent::tracked(""));

    Mem m;
    CHECK(!ent::ctx_ready(m, 0));
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x1000 + 0x228] = 0x2000;
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x2000 + 0x560] = 0x3000;
    m.words[0x2000 + 0x568] = 0x3000;
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x2000 + 0x568] = 0x3010;
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x1000 + 0x238] = 0;
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x1000 + 0x238] = 0x4000;
    CHECK(ent::ctx_ready(m, 0x1000));
    m.words[0x1000 + 0x238] = 0;
    CHECK(!ent::ctx_ready(m, 0x1000));
    m.words[0x1000 + 0x238] = 0x4000;
    m.words[0x1000 + 0x228] = 0;
    CHECK(!ent::ctx_ready(m, 0x1000));

    ent::Settle owner;
    CHECK(owner.state() == ent::UNKNOWN);
    CHECK(owner.read(true) && owner.state() == ent::OWNED);
    CHECK(!owner.read(false) && !owner.read(true) && owner.state() == ent::OWNED);

    ent::Settle none;
    for (int i = 0; i < ent::NOT_OWNED_AFTER - 1; i++)
        CHECK(!none.read(false) && none.state() == ent::UNKNOWN);
    CHECK(none.read(false) && none.state() == ent::NOT_OWNED);
    CHECK(!none.read(false) && none.state() == ent::NOT_OWNED);
    CHECK(none.read(true) && none.state() == ent::OWNED);

    CHECK(ent::NOT_OWNED_AFTER >= 3600);

    ent::Settle late;
    for (int i = 0; i < ent::NOT_OWNED_AFTER - 1; i++)
        late.read(false);
    CHECK(late.read(true) && late.state() == ent::OWNED);

    CHECK(!ent::frontend_settled(0, 100000));
    CHECK(!ent::frontend_settled(1000, 1000 + ent::FRONTEND_SETTLE_MS - 1));
    CHECK(ent::frontend_settled(1000, 1000 + ent::FRONTEND_SETTLE_MS));
    CHECK(!ent::frontend_settled(5000, 4000));

    CHECK(ent::json(ent::UNKNOWN).empty());
    CHECK(ent::json(ent::OWNED) == "{\"f1_2026\": true}\n");
    CHECK(ent::json(ent::NOT_OWNED) == "{\"f1_2026\": false}\n");
    CHECK(std::string(ent::word(ent::NOT_OWNED)) == "not owned");

    std::puts("entitlements_test: ok");
    return 0;
}
