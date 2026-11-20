#include "core/registry/older.hpp"
#include "core/registry/table.hpp"

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

using namespace swofty_core::registry;
namespace older = swofty_core::registry::older;
using swofty::json::Value;

static int failures = 0;

static void check(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok" : "FAIL", what);
    if (!ok)
        failures++;
}

using Ops = std::vector<std::pair<std::string, Value>>;

static Value text(const std::string &s) { return Value::object({{"*", Value(s)}}); }

static std::string dump(Store &st, const std::vector<std::string> &keys)
{
    std::string out;
    for (auto &k : keys) {
        std::string id = "game/" + k;
        auto rec = st.get(older::STRING, id);
        out += id + "|" + std::to_string(int(rec.status)) + "|" + rec.owner + "|";
        for (auto &[lang, t] : older::string_texts(rec.value))
            out += lang + "=" + t + ",";
        for (auto &c : st.chain(older::STRING, id))
            out += ";" + c;
        out += "\n";
    }
    return out;
}

static void diff(const std::string &a, const std::string &b)
{
    size_t i = 0, shown = 0;
    while (i < a.size() && i < b.size() && shown < 3) {
        size_t ea = a.find('\n', i), eb = b.find('\n', i);
        if (a.compare(i, ea - i, b, i, eb - i) != 0 || ea != eb) {
            std::printf("  single: %s\n  batch : %s\n", a.substr(i, ea - i).c_str(), b.substr(i, eb - i).c_str());
            shown++;
        }
        if (ea != eb)
            break;
        i = ea + 1;
    }
}

static int provide_text(void *, const char *, const char *, char *out, size_t cap, size_t *len)
{
    static const char body[] = "{\"text\":{}}";
    if (len)
        *len = sizeof body - 1;
    if (!out || cap < sizeof body)
        return SWOFTY_TOO_SMALL;
    std::memcpy(out, body, sizeof body);
    return SWOFTY_ASSET_OK;
}

static void seed(Store &st, size_t existing, bool provide)
{
    older::declare(st);
    if (provide)
        st.provide_fn("game", older::STRING, &provide_text, nullptr, nullptr);
    st.end_register();
    Ops base;
    for (size_t i = 0; i < existing; i++)
        base.push_back({"K" + std::to_string(i), text("base" + std::to_string(i))});
    older::string_layers(st, "other.addon", base);
    Ops mine;
    for (size_t i = 0; i < existing; i += 7)
        mine.push_back({"K" + std::to_string(i), text("old" + std::to_string(i))});
    for (auto &[k, v] : mine)
        older::string_layer(st, "ex.addon", k, v);
}

static Ops build(size_t n)
{
    Ops ops;
    for (size_t i = 0; i < n; i++) {
        std::string k = "K" + std::to_string(i * 2);
        if (i % 5 == 0)
            ops.push_back({k, Value()});
        ops.push_back({k, text("new" + std::to_string(i))});
        if (i % 11 == 0)
            ops.push_back({k, text("again" + std::to_string(i))});
        if (i % 13 == 0)
            ops.push_back({k, Value()});
    }
    return ops;
}

int main()
{
    const size_t EXISTING = 5000, KEYS = 2000;
    Ops ops = build(KEYS);
    std::vector<std::string> keys;
    for (size_t i = 0; i < EXISTING + 100; i++)
        keys.push_back("K" + std::to_string(i));

    Store single, batch;
    std::vector<std::string> single_log, batch_log;
    seed(single, EXISTING, false);
    seed(batch, EXISTING, true);
    single.set_log([&](const std::string &s) { single_log.push_back(s); });
    batch.set_log([&](const std::string &s) { batch_log.push_back(s); });

    using clock = std::chrono::steady_clock;
    auto t0 = clock::now();
    for (auto &[k, v] : ops)
        older::string_layer(single, "ex.addon", k, v);
    double single_ms = std::chrono::duration<double, std::milli>(clock::now() - t0).count();
    auto t1 = clock::now();
    int rc = older::string_layers(batch, "ex.addon", ops);
    double batch_ms = std::chrono::duration<double, std::milli>(clock::now() - t1).count();

    std::printf("timing: %zu ops on %zu existing layers: single calls %.2f ms (%zu log lines), batch %.2f ms (%zu log lines)\n",
                ops.size(), EXISTING, single_ms, single_log.size(), batch_ms, batch_log.size());

    check(rc == SWOFTY_ASSET_OK, "the batch reports ok");
    diff(dump(single, keys), dump(batch, keys));
    check(dump(single, keys) == dump(batch, keys),
          "the batch resolves the same strings, owners and layer chains as single calls");
    check(single.layer_count("ex.addon", older::STRING) == batch.layer_count("ex.addon", older::STRING),
          "the batch leaves the same number of layers for the addon");
    check(single.layer_count("other.addon", older::STRING) == batch.layer_count("other.addon", older::STRING),
          "the batch leaves the other addon's layers alone");
    check(batch_log.size() == 1, "the batch logs one line");
    check(single_log.size() > 1000, "single calls log one line each");

    Ops clears;
    for (size_t i = 0; i < KEYS; i++) {
        clears.push_back({"K" + std::to_string(i * 2), Value()});
        if (i % 3 == 0)
            clears.push_back({"K" + std::to_string(i * 2 + 1), Value()});
    }
    for (auto &[k, v] : clears)
        older::string_layer(single, "ex.addon", k, v);
    older::string_layers(batch, "ex.addon", clears);
    check(dump(single, keys) == dump(batch, keys), "clearing as a batch matches clearing one by one");
    check(batch.layer_count("ex.addon", older::STRING) == single.layer_count("ex.addon", older::STRING),
          "the cleared layer counts match");

    Ops again = build(50);
    for (auto &[k, v] : again)
        older::string_layer(single, "ex.addon", k, v);
    older::string_layers(batch, "ex.addon", again);
    check(dump(single, keys) == dump(batch, keys), "setting again after a clear matches");

    return failures ? 1 : 0;
}
