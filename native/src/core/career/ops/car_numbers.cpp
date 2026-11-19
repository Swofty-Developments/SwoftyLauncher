#include "core/career/ops/ops_api.hpp"

#include <string>
#include <vector>

#include "core/career/ops/driver_rows.hpp"
#include "core/career/ops/number_plan.hpp"
#include "core/career/ops/lineup.hpp"
#include "core/career/ops/tables.hpp"

namespace career::ops::live {

constexpr uintptr_t kVtCurrentWeekendOf = 0xa8;
constexpr uintptr_t kWeekendRowsBegin = 0x20;
constexpr uintptr_t kWeekendRowsEnd = 0x28;

using CurrentWeekendFn = uintptr_t (*)(void *registry);

uintptr_t weekend_row(uintptr_t e, const EntrantRecord &row)
{
    uintptr_t registry = e + offsetof(g::CareerEntity, component_registry);
    uintptr_t vt = rdv<uintptr_t>(registry);
    uintptr_t fn = vt ? rdv<uintptr_t>(vt + kVtCurrentWeekendOf) : 0;
    if (!mem::in_image(fn))
        return 0;
    uintptr_t weekend = reinterpret_cast<CurrentWeekendFn>(fn)(reinterpret_cast<void *>(registry));
    uintptr_t b = weekend ? rdv<uintptr_t>(weekend + kWeekendRowsBegin) : 0;
    uintptr_t en = weekend ? rdv<uintptr_t>(weekend + kWeekendRowsEnd) : 0;
    if (!b || en < b || (en - b) % sizeof(EntrantRecord) || (en - b) / sizeof(EntrantRecord) > kMaxEntrants)
        return 0;
    for (uintptr_t p = b; p < en; p += sizeof(EntrantRecord)) {
        uint32_t v = rdv<uint32_t>(p + offsetof(EntrantRecord, ordinal_flags));
        bool want_human = row.ordinal_flags & kHumanBit;
        if (want_human ? (v & kHumanBit) != 0 : !(v & kHumanBit) && v == row.ordinal_flags)
            return driver_of(rdv<EntrantRecord>(p)) == driver_of(row) ? p : 0;
    }
    return 0;
}

bool write_number(uintptr_t e, const Row &row, uint8_t n, bool &weekend)
{
    weekend = false;
    auto set = reinterpret_cast<g::Career_Set_EntrantData_Thunk_t *>(bind("Career_Set_EntrantData_Thunk"));
    if (!set || entity_busy(e) || (!human(row.rec) && !unique_identity(e, row)))
        return false;
    uint8_t id[kIdentitySize];
    std::memcpy(id, &row.rec, kIdentitySize);
    EntrantWriteData d = own_data(row);
    set(reinterpret_cast<void *>(e), id, reinterpret_cast<const uint32_t *>(&d), row.rec.team_position, n);
    if (uintptr_t w = weekend_row(e, row.rec); w && writable(w + offsetof(EntrantRecord, car_number), 1)) {
        *reinterpret_cast<uint8_t *>(w + offsetof(EntrantRecord, car_number)) = n;
        weekend = true;
    }
    return true;
}

std::string number_text(uint8_t n) { return n == kNoNumber ? std::string("none") : "#" + std::to_string(n); }

int api_set_car_number(uint32_t driver, int32_t want_number, uint32_t flags)
{
    labels();
    const uint32_t stat = driver & 0x3fff, domain = (driver >> 14) & 3;
    std::string head = "set_car_number " + (domain ? "driver(" + std::to_string(driver) + ")" : driver_name(driver)) + " #" +
                       std::to_string(want_number) + ((flags & SWOFTY_NUMBER_SWAP) ? " (swap)" : "");
    std::string summary;
    int r = SWOFTY_ASSET_OK;
    uintptr_t e = 0;
    std::vector<Row> rows;
    if (!on_flow_thread()) {
        summary = "late: called off the flow thread";
        r = SWOFTY_ASSET_LATE;
    } else if (!career::ops::rows::driver_id(driver) || (flags & ~uint32_t(SWOFTY_NUMBER_SWAP))) {
        summary = "a driver save id (stat id 0..16382, the domain in bits 14-15) and flags 0 or SWOFTY_NUMBER_SWAP are needed";
        r = SWOFTY_ASSET_INVALID;
    } else if (!bind("Career_Set_EntrantData_Thunk")) {
        summary = "unavailable: binding(s) not resolved: Career_Set_EntrantData_Thunk";
        r = SWOFTY_ASSET_UNAVAILABLE;
    } else if (!(e = entity()) || !entrant_rows(e, rows)) {
        summary = "no career entity or its entrant rows are unreadable";
        r = SWOFTY_ASSET_LATE;
    } else if (entity_busy(e)) {
        summary = "the game is in the middle of an entrant update";
        r = SWOFTY_ASSET_LATE;
    } else {
        std::vector<career::ops::numbers::Row> view;
        size_t humans = 0;
        for (auto &row : rows) {
            view.push_back({driver_of(row.rec), row.rec.car_number, !teamless(row.rec)});
            humans += human(row.rec);
        }
        int idx = row_index(rows, stat, domain);
        uint32_t key = idx >= 0 ? driver_of(rows[idx].rec) : career::ops::rows::key_of_id(driver);
        career::ops::numbers::Plan np = career::ops::numbers::plan(view, key, want_number, flags & SWOFTY_NUMBER_SWAP);
        if (np.error != career::ops::numbers::Error::none) {
            summary = "refused: " + np.why;
            r = np.error == career::ops::numbers::Error::taken ? int(SWOFTY_REFUSED) : int(SWOFTY_ASSET_INVALID);
        } else if (np.unchanged) {
            summary = "already " + number_text(rows[np.row].rec.car_number);
        } else {
            Row row = rows[np.row];
            const Row *holder = np.swap_with >= 0 ? &rows[np.swap_with] : nullptr;
            Row other = holder ? *holder : Row{};
            uint8_t had = row.rec.car_number, want = static_cast<uint8_t>(want_number);
            bool unique = true;
            for (const Row *x : {&row, holder ? &other : nullptr})
                if (x && (human(x->rec) ? humans != 1 : !unique_identity(e, *x)))
                    unique = false;
            if (!unique) {
                summary = "refused: an entrant row's identity is not unique in the line-up; nothing written";
                r = SWOFTY_ASSET_INVALID;
            } else {
                bool week_a = false, week_b = false;
                bool ok = write_number(e, row, want, week_a);
                ok = ok && (!holder || write_number(e, other, had, week_b));
                std::vector<Row> now;
                ok = ok && entrant_rows(e, now) && now.size() == rows.size();
                for (size_t i = 0; ok && i < rows.size(); ++i) {
                    EntrantRecord expect = rows[i].rec;
                    if (rows[i].at == row.at)
                        expect.car_number = want;
                    else if (holder && rows[i].at == other.at)
                        expect.car_number = had;
                    ok = now[i].at == rows[i].at && std::memcmp(&now[i].rec, &expect, sizeof expect) == 0;
                }
                if (!ok) {
                    bool wa, wb;
                    bool back = (!holder || write_number(e, other, want, wb)) && write_number(e, row, had, wa);
                    summary = std::string("FAILED: the entrant rows did not read back with the new number; ") +
                              (back ? "restored the old numbers" : "COULD NOT restore every row, RELOAD THE SAVE without saving");
                    r = SWOFTY_ASSET_INVALID;
                } else {
                    summary = number_text(had) + " -> " + number_text(want) +
                              (holder ? "; " + driver_name(stat_of(driver_of(other.rec))) + " " + number_text(want) + " -> " +
                                            number_text(had)
                                      : std::string()) +
                              " (weekend row " + (week_a && (!holder || week_b) ? "updated" : "not found") + "; " +
                              std::to_string(rows.size() - (holder ? 2 : 1)) + " other row(s) unchanged)";
                }
            }
        }
    }
    swofty::log("[career-ops] %s: %s", head.c_str(), summary.c_str());
    return r;
}

}
