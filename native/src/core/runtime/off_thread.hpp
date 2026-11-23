#pragma once

#include <atomic>
#include <chrono>
#include <cstdint>
#include <functional>
#include <memory>
#include <mutex>
#include <thread>
#include <utility>

// Disk work the game thread needs, done on a thread of its own: a slow or stalling drive then holds up that thread and
// never a frame. The game thread calls want(), which never waits. With every_ms 0 the job runs once, on the first
// want(). Otherwise it runs again every every_ms while want() was called within the last keep_ms. The thread is made
// on the first want() and outlives the object, which only tells it to stop.
namespace runtime {

class OffThread {
public:
    OffThread(std::function<void()> job, uint64_t every_ms, uint64_t keep_ms)
        : s_(std::make_shared<Shared>(std::move(job), every_ms, keep_ms))
    {
    }
    OffThread(const OffThread &) = delete;
    OffThread &operator=(const OffThread &) = delete;
    ~OffThread() { s_->stop.store(true, std::memory_order_relaxed); }

    void want()
    {
        s_->wanted_at.store(now_ms(), std::memory_order_relaxed);
        if (!s_->started.exchange(true))
            std::thread([s = s_] { s->loop(); }).detach();
    }

    uint64_t runs() const { return s_->runs.load(std::memory_order_acquire); }

    static uint64_t now_ms()
    {
        return uint64_t(
            std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now().time_since_epoch()).count());
    }

private:
    static constexpr uint64_t NAP_MS = 50;

    struct Shared {
        Shared(std::function<void()> j, uint64_t every, uint64_t keep) : job(std::move(j)), every_ms(every), keep_ms(keep) {}

        void loop()
        {
            for (;;) {
                job();
                runs.fetch_add(1, std::memory_order_release);
                if (!every_ms)
                    return;
                uint64_t ran_at = now_ms();
                for (;;) {
                    if (stop.load(std::memory_order_relaxed))
                        return;
                    std::this_thread::sleep_for(std::chrono::milliseconds(NAP_MS));
                    uint64_t now = now_ms();
                    if (now - ran_at >= every_ms && now - wanted_at.load(std::memory_order_relaxed) < keep_ms)
                        break;
                }
            }
        }

        std::function<void()> job;
        uint64_t every_ms, keep_ms;
        std::atomic<bool> started{false}, stop{false};
        std::atomic<uint64_t> wanted_at{0}, runs{0};
    };

    std::shared_ptr<Shared> s_;
};

// The newest value a job made off the game thread, such as a file read and parsed there. take() starts the job and
// never waits on it.
template <class T> class OffThreadValue {
public:
    OffThreadValue(std::function<T()> make, uint64_t every_ms, uint64_t keep_ms)
        : v_(std::make_shared<Slot>()), work_(
                                            [v = v_, make = std::move(make)] {
                                                T next = make();
                                                std::lock_guard<std::mutex> g(v->lock);
                                                v->value = std::move(next);
                                                v->made++;
                                            },
                                            every_ms, keep_ms)
    {
    }

    // The newest value, when it is newer than the one seen counts (0: none taken yet); seen then counts it. False while
    // no run has finished or no newer one has.
    bool take(T &out, uint64_t &seen)
    {
        work_.want();
        std::lock_guard<std::mutex> g(v_->lock);
        if (v_->made == seen)
            return false;
        out = v_->value;
        seen = v_->made;
        return true;
    }

    void want() { work_.want(); }

private:
    struct Slot {
        std::mutex lock;
        T value{};
        uint64_t made = 0;
    };

    std::shared_ptr<Slot> v_;
    OffThread work_;
};

}
