#pragma once

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <functional>
#include <thread>
#include <vector>

namespace swofty_core::apply::content::par {

inline constexpr unsigned MAX_THREADS = 8;

inline void each(size_t n, const std::function<void(size_t)> &fn)
{
    unsigned hw = std::max(1u, std::thread::hardware_concurrency());
    size_t threads = std::min<size_t>({n, size_t(hw), size_t(MAX_THREADS)});
    if (threads < 2) {
        for (size_t i = 0; i < n; i++)
            fn(i);
        return;
    }
    std::atomic<size_t> next{0};
    auto work = [&] {
        for (size_t i; (i = next.fetch_add(1)) < n;)
            fn(i);
    };
    std::vector<std::thread> pool;
    for (size_t t = 1; t < threads; t++) {
        try {
            pool.emplace_back(work);
        } catch (...) {
            break;
        }
    }
    work();
    for (auto &t : pool)
        t.join();
}

}
