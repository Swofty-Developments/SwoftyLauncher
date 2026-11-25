#pragma once
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

// career::set_car_number's rules, moved from Career Mode Editor's drivers.number.set (its
// native/src/race_numbers.cpp plan()): pure logic over the career's entrant rows, so native/test builds it on
// the host. A number is 1..99 and free when no other row of the championship holds it; with swap the row
// holding it takes the driver's old number instead, which needs the driver to have one. A driver with several
// rows owns his seated row.

namespace career::ops::numbers {

constexpr uint8_t kNone = 0xff;

struct Row {
    uint32_t driver = 0;
    uint8_t number = kNone;
    bool team = false;
};

enum class Error { none, bad_value, no_entrant, taken, nothing_to_give };

struct Plan {
    Error error = Error::none;
    std::string why;
    int row = -1;
    int swap_with = -1;
    bool unchanged = false;
};

inline int own_row(const std::vector<Row> &rows, uint32_t driver)
{
    int seated = -1, first = -1;
    for (size_t i = 0; i < rows.size(); ++i) {
        if (rows[i].driver != driver)
            continue;
        if (first < 0)
            first = static_cast<int>(i);
        if (!rows[i].team)
            continue;
        if (seated >= 0)
            return -1;
        seated = static_cast<int>(i);
    }
    return seated >= 0 ? seated : first;
}

inline Plan plan(const std::vector<Row> &rows, uint32_t driver, double number, bool swap)
{
    Plan p;
    if (!(number >= 1 && number <= 99) || number != std::floor(number)) {
        p.error = Error::bad_value;
        p.why = "a racing number is an integer 1..99";
        return p;
    }
    auto want = static_cast<uint8_t>(number);
    p.row = own_row(rows, driver);
    if (p.row < 0) {
        p.error = Error::no_entrant;
        p.why = "the driver has no entrant row in this championship, so he has no racing number";
        return p;
    }
    if (rows[p.row].number == want) {
        p.unchanged = true;
        return p;
    }
    for (size_t i = 0; i < rows.size(); ++i) {
        if (rows[i].driver == driver || rows[i].number != want)
            continue;
        if (!swap) {
            p.error = Error::taken;
            p.why = "number " + std::to_string(want) + " belongs to another driver in this championship";
            return p;
        }
        if (rows[p.row].number == kNone) {
            p.error = Error::nothing_to_give;
            p.why = "the driver has no number to give in a swap";
            return p;
        }
        p.swap_with = static_cast<int>(i);
        return p;
    }
    return p;
}

}
