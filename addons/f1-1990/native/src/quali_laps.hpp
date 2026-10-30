// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <array>
#include <cstdint>

namespace f1990::qlaps {

constexpr int kLimit = 12;
constexpr double kMinGap = 45.0;
constexpr size_t kCars = 32;

constexpr uint32_t kOnTrack = 1;
constexpr uint32_t kEnteringPitLane = 2;
constexpr uint32_t kEnteringPitBox = 3;
constexpr uint32_t kInPitBox = 4;
constexpr uint32_t kLeavingPitBox = 7;
constexpr uint32_t kLeavingGarage = 13;
constexpr uint32_t kLeavingPitLane = 14;
constexpr uint32_t kDrivingThrough = 15;
constexpr uint32_t kUnknown = 0xffffffff;

inline bool qualifying_session(uint32_t id)
{
    return id >= 5 && id <= 14;
}

inline bool counts_crossing(uint32_t status)
{
    return status == kOnTrack || status == kDrivingThrough;
}

inline bool out_of_garage(uint32_t status)
{
    return status == kOnTrack || status == kLeavingPitBox || status == kLeavingGarage || status == kLeavingPitLane ||
           status == kDrivingThrough;
}

enum class Act { none, counted, last, send_in };

struct Car {
    int used = 0;
    double last = -1e18;
    uint32_t status = kUnknown;
    bool queued = false;
};

class Counter {
public:
    explicit Counter(int limit = kLimit) : limit_(limit) {}

    void reset() { cars_.fill(Car{}); }

    int limit() const { return limit_; }

    void set_limit(int limit) { limit_ = limit; }

    const Car &car(uint8_t v) const { return cars_[v % kCars]; }

    int left(uint8_t v) const { return limit_ - car(v).used; }

    Act status(uint8_t v, uint32_t status, double now)
    {
        if (v >= kCars)
            return Act::none;
        Car &c = cars_[v];
        uint32_t was = c.status;
        c.status = status;
        if (status == was)
            return Act::none;
        if (status == kLeavingPitBox && c.used >= limit_) {
            c.last = now;
            return Act::send_in;
        }
        return status == kLeavingGarage ? start(c, now) : Act::none;
    }

    Act crossing(uint8_t v, double now)
    {
        if (v >= kCars)
            return Act::none;
        Car &c = cars_[v];
        if (!counts_crossing(c.status) || now - c.last < kMinGap)
            return Act::none;
        return start(c, now);
    }

    bool queue(uint8_t v)
    {
        if (v >= kCars || cars_[v].queued)
            return false;
        cars_[v].queued = true;
        return true;
    }

    void sent(uint8_t v)
    {
        if (v < kCars)
            cars_[v].queued = false;
    }

private:
    Act start(Car &c, double now)
    {
        c.last = now;
        if (c.used >= limit_)
            return Act::send_in;
        return ++c.used == limit_ ? Act::last : Act::counted;
    }

    int limit_;
    std::array<Car, kCars> cars_{};
};

}
