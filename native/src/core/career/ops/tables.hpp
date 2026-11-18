#pragma once

#include <cstdint>
#include <map>
#include <mutex>
#include "swofty_api.h"
#include <string>
#include <unordered_map>
#include <vector>

// What the career operations keep between calls, under one lock: the data groups' slot teams (set_slots), the game
// database's driver, look and team labels (labels.cpp), and the teams a start-of-career apply_teams dressed
// (apply_teams.cpp).
namespace career::ops::live {

struct Labels {
    bool loaded = false;
    std::unordered_map<std::string, uint32_t> driver_by_label;
    std::unordered_map<std::string, uint32_t> look_by_label;
    std::unordered_map<uint32_t, std::string> driver_by_stat;
    std::unordered_map<uint32_t, std::string> team_by_stat;
};

struct StartWorn {
    int slot = 0;
    int64_t stat = -1;
    std::string row, team;
};

struct StartState {
    int year = 0;
    std::vector<StartWorn> worn;
};

struct Tables {
    std::mutex lock;
    std::map<std::string, std::vector<uint32_t>> slots;
    Labels labels;
    std::map<std::string, StartState> starts;
};

extern Tables tables;

bool db_int(const SwoftyDb *db, const std::string &path, int64_t &out);
bool db_text(const SwoftyDb *db, const std::string &path, std::string &out);
bool labels();
std::string driver_name(uint32_t stat);
std::string team_name(uint32_t stat);
std::string record_label_of(const char *registry, const std::string &id);
std::string person_driver_label(const std::string &ref);
int resolve_driver(const std::string &ref);
int resolve_look(const std::string &ref);
int resolve_team(const std::string &ref);

}
