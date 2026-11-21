#pragma once
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <iterator>
#include <string>
#include <unordered_map>
#include <vector>

namespace runtime::game_mem {

/* The bookkeeping of the core's writes into data bindings (game_mem.cpp, SwoftyCoreGameMem): which addon wrote
   which bytes of which binding, and the bytes that were there before, so restore puts the game's own values back.
   Pure logic, no Windows calls: game_mem.cpp does the protection change and the copy.

   A write is admitted when the binding is the size every earlier write of it declared (the addon's C++ type of
   the binding), the range lies inside that size and no other addon holds a byte of it. The first write of a
   range by one addon saves the bytes there; writing the same range again saves nothing more, so the saved bytes
   stay the game's. Restore replays the saved bytes in the reverse order of the writes, which gives the game's
   bytes back even where one addon's ranges overlap. */
class Ledger {
public:
    enum class Admit { ok, size_mismatch, out_of_bounds, held };

    struct Write {
        std::string id, owner;
        uintptr_t at = 0;
        std::vector<uint8_t> original;
    };

    // size: the size the caller declares for the binding (sizeof its type); offset, n: the bytes to write.
    // holder: on held, the addon that holds an overlapping range.
    Admit admit(const std::string &id, size_t size, size_t offset, size_t n, const std::string &owner, uintptr_t base,
                std::string *holder = nullptr) const
    {
        auto s = sizes_.find(id);
        if (s != sizes_.end() && s->second != size)
            return Admit::size_mismatch;
        if (!n || offset > size || n > size - offset)
            return Admit::out_of_bounds;
        uintptr_t lo = base + offset, hi = lo + n;
        for (const Write &w : writes_)
            if (w.owner != owner && w.at < hi && lo < w.at + w.original.size()) {
                if (holder)
                    *holder = w.owner;
                return Admit::held;
            }
        return Admit::ok;
    }

    // After an admitted write: keeps the size and, for a range this owner has not written yet, the bytes that
    // were there (current, n bytes, read before the write).
    void record(const std::string &id, size_t size, uintptr_t at, const uint8_t *current, size_t n, const std::string &owner)
    {
        sizes_.emplace(id, size);
        for (const Write &w : writes_)
            if (w.owner == owner && w.id == id && w.at == at && w.original.size() == n)
                return;
        writes_.push_back({id, owner, at, std::vector<uint8_t>(current, current + n)});
    }

    // Takes the owner's writes of id (every binding when id is empty) out of the ledger, newest first: the order
    // to copy their saved bytes back in.
    std::vector<Write> take(const std::string &owner, const std::string &id)
    {
        std::vector<Write> out;
        for (size_t i = writes_.size(); i-- > 0;)
            if (writes_[i].owner == owner && (id.empty() || writes_[i].id == id)) {
                out.push_back(std::move(writes_[i]));
                writes_.erase(writes_.begin() + static_cast<std::ptrdiff_t>(i));
            }
        return out;
    }

    // Puts back writes taken from the ledger whose copy failed (newest first, as take gave them), so a later
    // restore tries them again.
    void put_back(std::vector<Write> failed)
    {
        for (size_t i = failed.size(); i-- > 0;)
            writes_.push_back(std::move(failed[i]));
    }

    // Forgets the declared size of every binding no write holds any more, so a restored binding can be written
    // again with another type.
    void prune_sizes()
    {
        for (auto s = sizes_.begin(); s != sizes_.end();) {
            bool used = false;
            for (const Write &w : writes_)
                used |= w.id == s->first;
            s = used ? std::next(s) : sizes_.erase(s);
        }
    }

    size_t count() const { return writes_.size(); }

private:
    std::unordered_map<std::string, size_t> sizes_;
    std::vector<Write> writes_;
};

/* A game allocator object (Ne allocator) is used through its vtable: +0x18 alloc(self, size, align) and +0x38
   free(self, p). An allocation the core makes is checked against these limits before the call. */
inline constexpr size_t alloc_slot = 0x18, free_slot = 0x38;
inline constexpr size_t max_alloc = size_t(1) << 30;

// size above 0 and at most max_alloc; align a power of two from 1 to 4096.
inline bool alloc_args_ok(size_t size, size_t align)
{
    return size && size <= max_alloc && align && align <= 4096 && !(align & (align - 1));
}

}
