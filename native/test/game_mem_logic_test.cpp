#include "core/runtime/game_mem_logic.hpp"

#include <cstdio>
#include <cstring>
#include <vector>

namespace {

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what);
        failures++;
    }
}

using runtime::game_mem::Ledger;

// A table of six u32 in a buffer standing in for the game's data section; write and restore do what
// game_mem.cpp does around the ledger.
struct Table {
    uint32_t v[6] = {0xffeead00, 0xff3ec58d, 0xff16deff, 0xff7a2aed, 0xffffffff, 0xffffffff};
    uintptr_t base() const { return reinterpret_cast<uintptr_t>(v); }
};

Ledger::Admit write(Ledger &l, Table &t, const char *owner, size_t index, uint32_t value, size_t size = sizeof(Table::v))
{
    size_t offset = index * sizeof(uint32_t);
    auto a = l.admit("k_Table", size, offset, sizeof value, owner, t.base());
    if (a != Ledger::Admit::ok)
        return a;
    uintptr_t at = t.base() + offset;
    std::vector<uint8_t> current(reinterpret_cast<uint8_t *>(at), reinterpret_cast<uint8_t *>(at) + sizeof value);
    std::memcpy(reinterpret_cast<void *>(at), &value, sizeof value);
    l.record("k_Table", size, at, current.data(), current.size(), owner);
    return a;
}

void restore(Ledger &l, const char *owner, const char *id = "")
{
    for (auto &w : l.take(owner, id))
        std::memcpy(reinterpret_cast<void *>(w.at), w.original.data(), w.original.size());
    l.prune_sizes();
}

void bounds_and_sizes()
{
    Ledger l;
    Table t;
    check(l.admit("k_Table", 24, 20, 4, "a", t.base()) == Ledger::Admit::ok, "last element fits");
    check(l.admit("k_Table", 24, 24, 4, "a", t.base()) == Ledger::Admit::out_of_bounds, "past the end");
    check(l.admit("k_Table", 24, 22, 4, "a", t.base()) == Ledger::Admit::out_of_bounds, "straddles the end");
    check(l.admit("k_Table", 24, 0, 0, "a", t.base()) == Ledger::Admit::out_of_bounds, "empty write");
    check(l.admit("k_Table", 24, ~size_t(0), 4, "a", t.base()) == Ledger::Admit::out_of_bounds, "offset wraps");
    check(l.admit("k_Table", 24, 4, ~size_t(0), "a", t.base()) == Ledger::Admit::out_of_bounds, "length wraps");
    check(write(l, t, "a", 4, 0xff00ff00) == Ledger::Admit::ok, "first write");
    check(write(l, t, "a", 5, 0xff00ff00, 32) == Ledger::Admit::size_mismatch, "another declared size");
    restore(l, "a");
    check(write(l, t, "a", 5, 0xff00ff00, 32) == Ledger::Admit::ok, "a restored binding takes a new size");
}

void restore_gives_stock_back()
{
    Ledger l;
    Table t, stock;
    check(write(l, t, "a", 4, 1) == Ledger::Admit::ok, "write [4]");
    check(write(l, t, "a", 4, 2) == Ledger::Admit::ok, "write [4] again");
    check(write(l, t, "a", 5, 3) == Ledger::Admit::ok, "write [5]");
    check(l.count() == 2, "the second write of [4] saves nothing");
    check(t.v[4] == 2 && t.v[5] == 3, "values written");
    restore(l, "a");
    check(!std::memcmp(t.v, stock.v, sizeof t.v), "restore gives the stock table back");
    check(l.count() == 0, "ledger empty after restore");
}

void owners_do_not_overlap()
{
    Ledger l;
    Table t, stock;
    std::string holder;
    check(write(l, t, "a", 4, 1) == Ledger::Admit::ok, "a writes [4]");
    check(l.admit("k_Table", 24, 16, 4, "b", t.base(), &holder) == Ledger::Admit::held && holder == "a", "b may not write [4]");
    check(l.admit("k_Table", 24, 14, 4, "b", t.base()) == Ledger::Admit::held, "b may not write half of [4]");
    check(write(l, t, "b", 5, 2) == Ledger::Admit::ok, "b writes [5]");
    restore(l, "b");
    check(t.v[4] == 1 && t.v[5] == stock.v[5], "b's restore leaves a's write");
    restore(l, "a", "k_Other");
    check(t.v[4] == 1, "a restore of another binding leaves k_Table");
    restore(l, "a", "k_Table");
    check(!std::memcmp(t.v, stock.v, sizeof t.v), "a's restore gives the stock table back");
}

void put_back_keeps_failed()
{
    Ledger l;
    Table t;
    write(l, t, "a", 1, 7);
    write(l, t, "a", 2, 8);
    auto taken = l.take("a", "");
    check(taken.size() == 2 && taken[0].at > taken[1].at, "take gives newest first");
    l.put_back(std::move(taken));
    check(l.count() == 2, "put_back keeps both");
    auto again = l.take("a", "");
    check(again.size() == 2 && again[0].at > again[1].at, "order kept through put_back");
}

void alloc_args()
{
    using runtime::game_mem::alloc_args_ok;
    check(alloc_args_ok(0x20, 4), "small aligned");
    check(alloc_args_ok(1, 1), "one byte");
    check(!alloc_args_ok(0, 8), "zero size");
    check(!alloc_args_ok(8, 0), "zero align");
    check(!alloc_args_ok(8, 12), "align not a power of two");
    check(!alloc_args_ok(8, 8192), "align above a page");
    check(!alloc_args_ok(runtime::game_mem::max_alloc + 1, 8), "too big");
}

}

int main()
{
    bounds_and_sizes();
    restore_gives_stock_back();
    owners_do_not_overlap();
    put_back_keeps_failed();
    alloc_args();
    if (failures)
        return 1;
    std::printf("game_mem_logic: ok\n");
    return 0;
}
