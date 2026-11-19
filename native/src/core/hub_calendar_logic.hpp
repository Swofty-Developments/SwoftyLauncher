#pragma once
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

// Career hub calendar diagnostic without the game. SessionStateCareerHub's onStateUpdate (0x142d51f70) leaves for
// Team HQ when the career calendar check 0x141265170 answers {1, valid}: it collects the human's calendar activities
// running today (Career_Collect_ActivitiesByStage 0x1407c6380: first day <= today <= last day, the list is sorted by
// first day) and the first one in state 1 whose type is in DECIDING_TYPES picks the answer from the table at
// 0x1456d0b20 (RaceWeekend stays, PressInterview 2, EndSeason 3, every other type Team HQ). No such activity is
// Team HQ too, and a season that is over answers 3 without the walk. The collector also drops one activity class by
// RTTI, which the core can't tell apart, so each item carries its vtable.
//
// A career session enters SessionStateCareerHub right after StateSession enters and leaves it in the same
// millisecond, healthy or not. The tell of the stuck-weekend crash is StateSession itself ending within FAST_END_MS of
// its start after the hub opened; SessionWatch reports that. A weekend whose progress state is past not started with
// no session complete is one the game saved as started without a session run (never_ran).

namespace runtime::hub_calendar {

constexpr uint32_t djb2(const char *s)
{
    uint32_t h = 5381;
    for (; *s; ++s)
        h = h * 33 + static_cast<uint8_t>(*s);
    return h;
}

constexpr uint32_t HUB = djb2("SessionStateCareerHub");
static_assert(HUB == 0x42843edbu, "state name hashing drifted");
constexpr uint32_t SESSION = djb2("StateSession");
static_assert(SESSION == 0x1505f42au, "state name hashing drifted");
constexpr uint32_t UNLOAD = djb2("StateUnloadSession");
static_assert(UNLOAD == 0xd432de2du, "state name hashing drifted");

constexpr uint64_t FAST_END_MS = 2000;
constexpr uint32_t WEEKEND_NOT_STARTED = 0;
constexpr uint32_t WEEKEND_IN_PROGRESS = 1;
constexpr uint32_t WEEKEND_COMPLETE = 2;
constexpr uint32_t MINUTES_PER_DAY = 1440;
constexpr size_t MAX_ITEMS = 512;
constexpr uint64_t SLOT_SIZE = 16;
constexpr uint64_t SLOT_ITEM = 8;
constexpr uint8_t RACE_WEEKEND = 0x0a;
constexpr uint8_t CONTRACT_EVENT = 0x0f;
constexpr uint8_t SECRET_MEETING = 0x10;
constexpr uint8_t TYPE_COUNT = 0x23;
constexpr uint64_t DECIDING_TYPES = 0x7ffffbebbull;
constexpr uint16_t NO_ID = 0xffff;

inline constexpr const char *TYPE_NAMES[TYPE_COUNT] = {
    "ContractDriverRenewal", "ContractTeamRenewal", "ContractTransferWindow", "CarReveal", "GridReveal", "EndSeason",
    "FreeTime", "Invitational", "NoActivity", "PressInterview", "RaceWeekend", "DynamicBreak", "SeasonBreak",
    "SponsorRenewal", "StartSeason", "NCSContractRenewal", "NSCSSM", "YDScout", "YDContractRenewal", "YDSigned",
    "SlidingScaleRefresh", "ContractSponsorResponse", "ContractNegotiation", "ContractFinalOffer", "CostCapBudget",
    "CostCapInfo", "CostCapBreachingCap", "DriverContractWindowOpen", "DriverContractAITeamApproach",
    "DriverContractAccepted", "DriverContractDeclined", "DriverContractTalks", "DriverContractSecretDriverApproach",
    "DriverContractAISignedDriver", "DriverContractTalksReminder"};

inline const char *type_name(uint8_t t)
{
    return t < TYPE_COUNT ? TYPE_NAMES[t] : "unknown";
}

struct Item {
    uint64_t at = 0, vtable = 0, def = 0;
    uint16_t first = 0, last = 0, id = NO_ID;
    uint8_t type = 0xff, state = 0;
};

struct Layout {
    uint64_t list_begin, list_end;
    uint64_t item_day, item_state, item_def;
    uint64_t def_id, def_type, def_duration;
};

using Read = bool (*)(uint64_t at, void *out, size_t n, void *user);

template <class T> bool get(Read rd, void *user, uint64_t at, T &out)
{
    return at && rd(at, &out, sizeof(T), user);
}

inline bool running(const Item &i, uint16_t day)
{
    return i.first <= day && day <= i.last;
}

inline bool deciding(uint8_t type)
{
    return type < TYPE_COUNT && ((DECIDING_TYPES >> type) & 1);
}

inline bool logs_add(uint8_t type)
{
    return type == CONTRACT_EVENT || type == SECRET_MEETING;
}

enum class Verdict : uint8_t { stay = 0, team_hq = 1, press = 2, season_end = 3 };

inline Verdict verdict_of(uint8_t type)
{
    return type == RACE_WEEKEND ? Verdict::stay
           : type == 0x09       ? Verdict::press
           : type == 0x05       ? Verdict::season_end
                                : Verdict::team_hq;
}

inline const char *verdict_name(Verdict v)
{
    switch (v) {
    case Verdict::stay:
        return "stay in the hub";
    case Verdict::team_hq:
        return "go to Team HQ";
    case Verdict::press:
        return "go to the press interview";
    case Verdict::season_end:
        return "go to the end of the season";
    }
    return "?";
}

struct Decision {
    Verdict verdict = Verdict::team_hq;
    int by = -1;
};

inline Decision decide(const std::vector<Item> &today, bool season_over)
{
    if (season_over)
        return {Verdict::season_end, -1};
    for (size_t i = 0; i < today.size(); ++i) {
        const Item &it = today[i];
        if (it.state == 1 && it.def && it.id != NO_ID && deciding(it.type))
            return {verdict_of(it.type), int(i)};
    }
    return {Verdict::team_hq, -1};
}

inline bool read_item(uint64_t at, const Layout &L, Read rd, void *user, Item &out)
{
    out = Item{};
    out.at = at;
    if (!get(rd, user, at, out.vtable) || !get(rd, user, at + L.item_day, out.first))
        return false;
    get(rd, user, at + L.item_state, out.state);
    out.last = out.first;
    if (get(rd, user, at + L.item_def, out.def) && out.def) {
        uint16_t duration = 0;
        get(rd, user, out.def + L.def_id, out.id);
        get(rd, user, out.def + L.def_type, out.type);
        if (get(rd, user, out.def + L.def_duration, duration) && duration)
            out.last = uint16_t(out.first + duration - 1);
    }
    return true;
}

// The calendar's activities running on day, in calendar order; false when the list itself doesn't read.
inline bool collect(uint64_t calendar, uint16_t day, const Layout &L, Read rd, void *user, std::vector<Item> &out,
                    size_t *total = nullptr)
{
    out.clear();
    uint64_t b = 0, e = 0;
    if (!get(rd, user, calendar + L.list_begin, b) || !get(rd, user, calendar + L.list_end, e) || !b || e < b ||
        (e - b) % SLOT_SIZE || (e - b) / SLOT_SIZE > MAX_ITEMS)
        return false;
    if (total)
        *total = size_t((e - b) / SLOT_SIZE);
    for (uint64_t slot = b; slot < e; slot += SLOT_SIZE) {
        uint64_t at = 0;
        Item it;
        if (!get(rd, user, slot + SLOT_ITEM, at) || !at || !read_item(at, L, rd, user, it))
            continue;
        if (it.first > day)
            break;
        if (running(it, day))
            out.push_back(it);
    }
    return true;
}

inline std::string describe(const Item &i)
{
    char b[224];
    snprintf(b, sizeof b, "%s (0x%02x) days %u-%u state %u id %d at 0x%llx vt 0x%llx", type_name(i.type), i.type,
             unsigned(i.first), unsigned(i.last), unsigned(i.state), i.id == NO_ID ? -1 : int(i.id),
             static_cast<unsigned long long>(i.at), static_cast<unsigned long long>(i.vtable));
    return b;
}

inline std::string describe(const Decision &d, const std::vector<Item> &today, bool season_over)
{
    std::string s = verdict_name(d.verdict);
    if (season_over)
        return s + " (the season is over)";
    if (d.by < 0)
        return s + " (no running activity decides)";
    char b[96];
    snprintf(b, sizeof b, " (item %d, %s, decides)", d.by + 1, type_name(today[size_t(d.by)].type));
    return s + b;
}

// One session from StateSession entering to its end: ended() reports a session whose career hub opened and that
// ended within FAST_END_MS of its start, with the milliseconds from the start to the end and to the hub.
class SessionWatch {
public:
    void started(uint64_t now)
    {
        start_ = now;
        open_ = true;
        hub_ = false;
    }

    void hub(uint64_t now)
    {
        if (!open_ || hub_)
            return;
        hub_ = true;
        hub_at_ = now;
    }

    bool ended(uint64_t now, uint64_t &ms, uint64_t &hub_ms)
    {
        if (!open_)
            return false;
        open_ = false;
        ms = now >= start_ ? now - start_ : 0;
        hub_ms = hub_at_ >= start_ ? hub_at_ - start_ : 0;
        return hub_ && ms < FAST_END_MS;
    }

private:
    uint64_t start_ = 0, hub_at_ = 0;
    bool open_ = false, hub_ = false;
};

// The current championship weekend as the core reads it (moments::career_weekend).
struct Weekend {
    uint32_t round = 0, rounds = 0, progress = 0, current = 0, sessions = 0, complete = 0;
    std::string session, track;
};

inline bool never_ran(const Weekend &w)
{
    return w.sessions && w.progress != WEEKEND_NOT_STARTED && w.complete == 0;
}

inline std::string progress_name(uint32_t p)
{
    if (p == WEEKEND_NOT_STARTED)
        return "not started";
    if (p == WEEKEND_IN_PROGRESS)
        return "in progress";
    if (p == WEEKEND_COMPLETE)
        return "complete";
    return "progress state " + std::to_string(p);
}

inline std::string describe(const Weekend &w)
{
    auto n = [](uint32_t v) { return std::to_string(v); };
    std::string s = "round " + n(w.round) + "/" + n(w.rounds) + ", weekend " + progress_name(w.progress);
    if (w.sessions)
        s += ", current session " + n(w.current + 1) + " of " + n(w.sessions) + " (" +
             (w.session.empty() ? "unknown" : w.session) + "), " + n(w.complete) + " of " + n(w.sessions) +
             " sessions complete";
    else
        s += ", its sessions do not read";
    return s + ", track " + (w.track.empty() ? "unknown" : w.track);
}

// One line per added activity of the logged types, not one per identical repeat (the daily tick adds the contract
// event to every calendar).
class AddLog {
public:
    bool fresh(uint64_t calendar, uint8_t type, uint32_t day)
    {
        if (calendar == calendar_ && type == type_ && day == day_)
            return false;
        calendar_ = calendar, type_ = type, day_ = day;
        return true;
    }

private:
    uint64_t calendar_ = 0;
    uint8_t type_ = 0;
    uint32_t day_ = UINT32_MAX;
};

}
