#pragma once
#include <cstddef>
#include <cstdint>
#include <vector>

namespace runtime {

/* VirtualQuery of an address in the game's image costs 0.1 to 1 ms on Windows: the kernel walks the page table
   from that page to the end of the region, and the code section is one region of about 90 MB. A binding pass
   asks for thousands of addresses in a few regions, so it keeps each answer for the region it describes and
   answers later addresses in that region from it, exactly as a fresh VirtualQuery would while no protection
   changes. Only a pass that installs nothing may keep answers. */
class ProtectionSpans {
public:
    enum class Answer { unknown, readable, refused };

    static constexpr size_t capacity = 512;

    /* VirtualQuery describes the region from the page of the queried address to the region's end, so the n
       bytes at p read only when that region is readable and they end inside it. */
    Answer lookup(uintptr_t p, size_t n) const
    {
        for (const Span &s : spans)
            if (p >= s.lo && p < s.hi)
                return s.readable && p + n <= s.hi ? Answer::readable : Answer::refused;
        return Answer::unknown;
    }

    void remember(uintptr_t lo, uintptr_t hi, bool readable)
    {
        if (hi <= lo)
            return;
        for (Span &s : spans)
            if (s.hi == hi && s.readable == readable) {
                if (lo < s.lo)
                    s.lo = lo;
                return;
            }
        if (spans.size() < capacity)
            spans.push_back({lo, hi, readable});
    }

    size_t size() const { return spans.size(); }

private:
    struct Span {
        uintptr_t lo, hi;
        bool readable;
    };
    std::vector<Span> spans;
};

}
