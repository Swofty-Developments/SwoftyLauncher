#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <mutex>
#include <vector>

namespace swofty_core::guards {

enum class Say : uint8_t { none, line, last };

constexpr Say say(int n, int first)
{
    if (n < 1 || n > first)
        return Say::none;
    return n == first ? Say::last : Say::line;
}

// One noted occurrence: its number, and what its log line ends with, or null when it is not logged.
struct Line {
    int n;
    const char *tail;

    explicit operator bool() const { return tail != nullptr; }
};

// " (further <what> are not logged)", which the last logged line of a cap ends with; empty for a cap without a noun.
class Tail {
public:
    constexpr Tail() = default;

    template <size_t N> constexpr explicit Tail(const char (&what)[N])
    {
        static_assert(sizeof PREFIX - 1 + N - 1 + sizeof SUFFIX <= sizeof text_);
        size_t at = 0;
        for (size_t i = 0; i + 1 < sizeof PREFIX; i++)
            text_[at++] = PREFIX[i];
        for (size_t i = 0; i + 1 < N; i++)
            text_[at++] = what[i];
        for (size_t i = 0; i + 1 < sizeof SUFFIX; i++)
            text_[at++] = SUFFIX[i];
        text_[at] = 0;
    }

    const char *text() const { return text_; }

    const char *at(int n, int first) const { return n > first ? nullptr : n == first ? text_ : ""; }

private:
    static constexpr char PREFIX[] = " (further ";
    static constexpr char SUFFIX[] = " are not logged)";

    char text_[48] = {};
};

// Logs the first First occurrences; the last of them ends with the tail.
template <int First> class LogFirst {
public:
    constexpr LogFirst() = default;

    template <size_t N> constexpr explicit LogFirst(const char (&what)[N]) : tail_(what) {}

    Line note()
    {
        int n = ++count_;
        return {n, tail_.at(n, First)};
    }

    int count() const { return count_.load(); }

    const char *tail() const { return tail_.text(); }

private:
    std::atomic<int> count_{0};
    Tail tail_;
};

// Logs the first occurrence of each of the first First distinct keys; the last of them ends with the tail. A key
// noted before, and every new key past the cap, is not logged and not kept.
template <class Key, size_t First> class FirstKeys {
public:
    constexpr FirstKeys() = default;

    template <size_t N> constexpr explicit FirstKeys(const char (&what)[N]) : tail_(what) {}

    Line note(const Key &key)
    {
        std::lock_guard<std::mutex> hold(lock_);
        for (const Key &k : seen_)
            if (k == key)
                return {0, nullptr};
        if (seen_.size() >= First)
            return {0, nullptr};
        seen_.push_back(key);
        int n = int(seen_.size());
        return {n, tail_.at(n, int(First))};
    }

    size_t size() const
    {
        std::lock_guard<std::mutex> hold(lock_);
        return seen_.size();
    }

    // Forgets every noted key: the next keys are noted, and logged, as the first again.
    void clear()
    {
        std::lock_guard<std::mutex> hold(lock_);
        seen_.clear();
    }

    const char *tail() const { return tail_.text(); }

private:
    mutable std::mutex lock_;
    std::vector<Key> seen_;
    Tail tail_;
};

// Whether a one-byte key (a track id) is noted for the first time. It keeps all 256 keys, has no cap and no tail, and
// is not synchronised: its holder locks around it.
class FirstBytes {
public:
    bool first(uint8_t key)
    {
        if (seen_[key])
            return false;
        seen_[key] = true;
        return true;
    }

private:
    bool seen_[256] = {};
};

}
