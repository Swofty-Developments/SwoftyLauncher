#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include "core/career/ops/entrants.hpp"
#include "core/career/ops/lineup_plan.hpp"
#include "core/runtime/json.hpp"

// The line-up executor (lineup.cpp): a Lineup of moves, repositions, numbers, cars and looks run as one batch of
// entrant writes, read back, and reported as a Report.
namespace career::ops::live {

std::string quote(const std::string &s);

struct Report {
    std::string result = "ok";
    size_t changed = 0, unchanged = 0, refused = 0, written = 0, numbers = 0, contracts = 0, released = 0;
    std::vector<std::string> refusals;
    std::string finalise = "not needed";
    uint64_t changes = 0;
    std::string extra;

    std::string json() const
    {
        std::string r = "{\"result\":" + quote(result) + ",\"changed\":" + std::to_string(changed) +
                        ",\"unchanged\":" + std::to_string(unchanged) + ",\"refused\":" + std::to_string(refused) +
                        ",\"refusals\":[";
        for (size_t i = 0; i < refusals.size(); ++i)
            r += (i ? "," : "") + quote(refusals[i]);
        r += "],\"written\":" + std::to_string(written) + ",\"released\":" + std::to_string(released) +
             ",\"numbers\":" + std::to_string(numbers) + ",\"finalise\":" + quote(finalise) +
             ",\"line_up_changes\":" + std::to_string(changes) + ",\"contracts_signed\":" + std::to_string(contracts) + extra +
             "}";
        return r;
    }
};

struct Lineup {
    int year = 0;
    std::string why;
    bool visuals = false;
    int player_team = -1;
    struct Move {
        uint32_t driver;
        int team, seat, car;
        int holder = -1, mate = -1;
    };
    struct Reposition {
        uint32_t driver;
        int seat;
    };
    struct Value {
        uint32_t driver;
        int value;
    };
    std::vector<Move> moves;
    std::vector<Reposition> repositions;
    std::vector<uint32_t> releases;
    std::vector<Value> numbers, cars, looks;
};

int put(const std::string &text, char *out, size_t cap, size_t *len);
int number(const json::Value &v, int def = -1);
bool read_lineup(const json::Value &j, Lineup &l, std::string &err);
int row_index(const std::vector<Row> &rows, uint32_t stat, uint32_t domain = 0);
int holder_of(const std::vector<Row> &rows, uint32_t team, int seat);
std::string seat_text(uint32_t team, int seat);
int execute(const Lineup &p, Report &rep, std::string &summary);
std::vector<career::ops::plan::Row> plan_rows(const std::vector<Row> &rows);
bool read_season(const json::Value &s, career::ops::plan::Input &in, int &year, std::string &err);

}
