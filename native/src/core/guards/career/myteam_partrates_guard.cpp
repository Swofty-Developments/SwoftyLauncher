#include "core/guards/career/myteam_partrates_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/runtime/loader/hook_install.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <string>

// My Team part rates guard (re/db/functions.tsv Game_EngineMgmt_InitPlayerPartRates, F1_25.exe+0x11cce90 on Steam build
// 26566308).
//
// Game_EngineMgmt_InitPlayerPartRates runs from Career_MyTeamSetup_AdvanceFadeOut as a new My Team career begins. It gives
// every entrant a part-rate record (engine management +0x2a0..+0x2a8, 0xa60 bytes each), finds the entrant that matches the local
// user's identity (the root at 0x14604e2c8, +0x140 +0x198 +0x1c0), and looks that entrant's
// key up in the records: r15 is the record, 0 when no entrant matches (the key then stays 0x3fff0000, the empty key) or
// the key has no record. The loop over the player's seven parts then reads [r15+0x80] with no check, although it tests
// everything after that load for null: an ACCESS_VIOLATION reading 0x80 at F1_25.exe+0x11cdd37 (r15 0, rbx 0, rdi
// 0x3fff0000) ends the career creation right after the team creation screens.
//
// The faulting load is followed by the game's own test of rax, whose null branch skips the car's part rate scaling for
// that part (0x1411cdd60, 0x29 past the load) and goes on to the next part. A null record resumes there, so the player's
// part rates keep the values the record-less path gives them. The first occurrences log the registers, the part-rate
// record keys and a census of the career's entrants (humans, entrants per car kind), the same census the My Team heal logs
// as the career loads, so the line shows which entrant the local player lacks.
//
// The site is found by byte scan.
//
// SWOFTY_MYTEAM_PARTRATES_GUARD=0 turns it off.

namespace {

namespace guards = swofty_core::guards;
namespace mem = swofty::mem;
namespace mp = runtime::myteam_partrates;

const runtime::Sig S_RECORD_LOAD{"site_Game_EngineMgmt_InitPlayerPartRates_RecordLoad",
                                 "49 8B 87 80 00 00 00 48 85 C0 74 1D 48 8B 40 58 48 85 C0 74 14 48 8B 80 30 05 00 00",
                                 0x11cdd37,
                                 runtime::Sig::addr,
                                 0,
                                 0,
                                 0};

struct Game {
    bool u64(uint64_t a, uint64_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }

    bool u32(uint64_t a, uint32_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }
};

uintptr_t resume_at = 0;
std::atomic<int> said{0};
safetyhook::MidHook h_record;

void record_load(safetyhook::Context &c)
{
    if (!mp::skips(c.r15))
        return;
    c.rax = 0;
    c.rip = resume_at;
    if (said.fetch_add(1) >= mp::LOG_LIMIT)
        return;
    Game game;
    mp::Registers r;
    r.engine = c.rsi;
    r.matched = c.rbx;
    r.key = c.rdi;
    r.ruleset = c.r12;
    swofty::log("[myteam] %s", mp::describe(game, r).c_str());
}

void setup()
{
    char v[16] = {};
    if (guards::env_off("SWOFTY_MYTEAM_PARTRATES_GUARD", v)) {
        swofty::log("[myteam] part rates guard off (SWOFTY_MYTEAM_PARTRATES_GUARD=%s)", v);
        return;
    }
    uintptr_t at = runtime::find(S_RECORD_LOAD, "career", "career");
    if (!at)
        return;
    resume_at = at + mp::RESUME;
    runtime::loader::Installed r = runtime::loader::install(h_record, at, S_RECORD_LOAD.what, record_load);
    if (!r) {
        swofty::log("[myteam] part rates guard: the hook failed: %s", r.error.c_str());
        return;
    }
    swofty::log("[myteam] part rates guard armed: a new My Team career whose local player has no part-rate record skips the "
                "player's part rates instead of faulting");
}

}

SWOFTY_HOOK_GUARD(myteam_partrates, "my team part rates guard", setup)
