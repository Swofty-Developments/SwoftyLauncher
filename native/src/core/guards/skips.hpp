#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>

namespace swofty_core::guards {

enum class Say : uint8_t { none, line, last };

constexpr Say say(int n, int first)
{
    if (n < 1 || n > first)
        return Say::none;
    return n == first ? Say::last : Say::line;
}

template <int First>
struct Skips {
    int count = 0;

    int note()
    {
        return ++count;
    }

    bool logged(int n) const
    {
        return n <= First;
    }

    bool last(int n) const
    {
        return n == First;
    }
};

template <int First>
class LogFirst {
public:
    struct Line {
        int n;
        const char *tail;

        explicit operator bool() const
        {
            return tail != nullptr;
        }
    };

    constexpr LogFirst() = default;

    template <size_t N>
    constexpr explicit LogFirst(const char (&what)[N])
    {
        static_assert(sizeof PREFIX - 1 + N - 1 + sizeof SUFFIX <= sizeof tail_);
        size_t at = 0;
        for (size_t i = 0; i + 1 < sizeof PREFIX; i++)
            tail_[at++] = PREFIX[i];
        for (size_t i = 0; i + 1 < N; i++)
            tail_[at++] = what[i];
        for (size_t i = 0; i + 1 < sizeof SUFFIX; i++)
            tail_[at++] = SUFFIX[i];
        tail_[at] = 0;
    }

    Line note()
    {
        int n = ++count_;
        return {n, n > First ? nullptr : n == First ? tail_ : ""};
    }

    int count() const
    {
        return count_.load();
    }

    const char *tail() const
    {
        return tail_;
    }

private:
    static constexpr char PREFIX[] = " (further ";
    static constexpr char SUFFIX[] = " are not logged)";

    std::atomic<int> count_{0};
    char tail_[48] = {};
};

}
