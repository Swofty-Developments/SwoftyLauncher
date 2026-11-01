// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: economy (protocol/economy.md). Cash (My Team) and resource points (both
// modes), plus the `economy` snapshot section.
//
// Cash (vt_Career_BankTransactionMessage, swofty::career::post). The team's
// money is a CareerBankComponent account (CareerBankComponent.team_accounts, BankAccount keyed by
// key = u8 kind | u16 team stat id << 16, BankSubAccount with category 1 = team cash, balance,
// minimum and a BankTransaction history). Career_Get_BankTeamBalance (the player card's cash) is the
// sum of the sub-account balances; it is a leaf without .pdata, so this file reads the same fields.
// The edit posts a BankTransactionMessage on the game bus, built exactly as the game's own posters
// build it on the stack (0x140d6b750, 0x1411243c0, 0x140f898c0). The post is synchronous: the
// bank's handler (0x140c68310) resolves the day (flow 0 = today), skips the affordability check
// (check_funds 0), and applies it (0x140c635c0 -> 0x140c679d0 -> 0x140c66c30): the balance
// saturates at INT32_MAX, the history gets a record, income is split into the budget pools, and
// the "transaction applied" message refreshes the Team HQ UI. eMyTeamReward is excluded from the
// owner-perk income multiplier (mask 0xc208040 in 0x140c635c0), so the amount lands exactly.
//
// Resource points (Career_Get_RpWallet, Career_Adjust_RpDriverWallet,
// Career_Adjust_RpTeamWallet). ResourcePointsComponent keeps the spendable wallets: {wallet_entity,
// wallet_mode} is the view the getter takes (mode 0 = one wallet per driver in driver_wallets, 1 = one
// per team in team_wallets). The adjust functions are the calls the game's own award handlers end
// in (MessageResourcePointsAwarded 0x14122a070, resource_points_generation_message 0x141229e00,
// MessagePassiveObjectiveAwarded 0x141229f40): add, clamp at 0, leave a locked (unlimited) wallet
// alone, post resource_points_adjusted_message for the UI. Only the player's wallet is touched.
//
// Every field comes from the KB structs (swofty::game); messages go out through the core's career
// library (swofty::career::post, the game's Ne_Post_BusMessage on the live bus) and guarded reads
// through swofty/mem.hpp (via ce::comp). Constants left here are element caps, enum values, vtable
// slots and hash-map control bytes.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include <swofty/career.hpp>
#include <swofty/game.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "ce.hpp"
#include "components.hpp"

using namespace swofty::game;
namespace comp = ce::comp;

namespace {

constexpr size_t kMaxAccounts = 256;
constexpr size_t kMaxSubs = 64;
constexpr size_t kMaxHistory = 1u << 20;
constexpr size_t kHistoryShown = 20;
constexpr uint32_t kMinutesPerDay = 1440;

constexpr uint8_t kCategoryTeamCash = 1;
constexpr uint8_t kTypeDebug = 8;
constexpr uint8_t kTypeMyTeamReward = 27;
constexpr uint8_t kFlowImmediate = 0;

// The adjust and get functions read the embedded wallet stores' hash-map fields up to +0x64 past
// the store start; the struct only models each store's first qword.
constexpr size_t kWalletStoreBytes = 0x78;
constexpr uint32_t kNoKey = 0x3fff0000u;

constexpr int64_t kCashMax = 2147483647;
constexpr int64_t kRpMax = 999999;

const char *const kTxTypeNames[] = {
    "eNone", "eSeasonPrize", "eCorporateFacilityIncome", "eSponsorSigningBonus",
    "ePrimarySponsorRegularPayment", "eSecondarySponsorRegularPayment", "eDamageRefund", "eActivityIncoming",
    "eDebug", "eEngineDeal", "ePersonalDevelopmentUpgrade", "eFacilityRunningCost",
    "eFacilityUpgradeEngineering", "eFacilityUpgradePersonnel", "eFacilityUpgradeCorporate",
    "eFacilityUpgradeOnboardingGift", "eDriverSigningBonus", "eDriverSalary", "eDriverClausePenalty",
    "eActivityOutgoing", "eDriverContractNegotiation", "eInitialCash", "ePrimarySponsorObjectives",
    "eInvitationalReward", "eSecondaySponsorObjectives", "eDepartmentEvent", "eEntryStatusBonus",
    "eMyTeamReward", "eUpgradeDevelopment", "eSponsorPressInterviewPayout", "eFacilityUpgrade",
};

template <class T> T rd(uintptr_t a, T def = T{})
{
    T v{};
    return comp::read(a, v) ? v : def;
}

bool same_key(uint32_t a, uint32_t b)
{
    return (a >> 16) == (b >> 16) && (a & 0xff) == (b & 0xff);
}

ce::OpResult fail(const std::string &error, const std::string &after = {})
{
    ce::OpResult r;
    r.error = error;
    r.after = after;
    return r;
}

bool arg_int(const ce::Json &args, const char *key, int64_t lo, int64_t hi, int64_t &out)
{
    const ce::Json &v = args[key];
    if (v.type != ce::Json::Num || v.n != static_cast<double>(static_cast<int64_t>(v.n)))
        return false;
    out = static_cast<int64_t>(v.n);
    return out >= lo && out <= hi;
}

bool expect_ok(const ce::Json &args, int64_t live)
{
    const ce::Json &v = args["expect"];
    if (v.type == ce::Json::Null)
        return true;
    if (v.type == ce::Json::Num)
        return static_cast<int64_t>(v.n) == live;
    if (v.type == ce::Json::Obj) {
        for (const char *k : {"cash", "rp", "value"})
            if (v[k].type == ce::Json::Num)
                return static_cast<int64_t>(v[k].n) == live;
    }
    return false;
}

const char *mode_name()
{
    switch (ce::mode()) {
    case ce::Mode::MyTeam:
        return "my_team";
    case ce::Mode::DriverCareer:
        return "driver_career";
    default:
        return "none";
    }
}

// ---------------------------------------------------------------------------
// Bank (CareerBankComponent)

struct Account {
    const BankAccount *at = nullptr;
    const BankSubAccount *subs = nullptr;
    size_t nsubs = 0;
};

bool find_account(uintptr_t bank_ptr, uint32_t key, Account &a)
{
    auto *bank = comp::view<CareerBankComponent>(bank_ptr);
    size_t n = 0;
    if (!bank || !comp::array(bank->team_accounts_begin, bank->team_accounts_end, kMaxAccounts, n))
        return false;
    for (size_t i = 0; i < n; ++i) {
        const BankAccount &acct = bank->team_accounts_begin[i];
        if (!same_key(acct.key, key))
            continue;
        size_t ns = 0;
        if (!comp::array(acct.subs_begin, acct.subs_end, kMaxSubs, ns))
            return false;
        a.at = &acct;
        a.subs = acct.subs_begin;
        a.nsubs = ns;
        return true;
    }
    return false;
}

int64_t balance(const Account &a)
{
    int64_t sum = 0;
    for (size_t i = 0; i < a.nsubs; ++i)
        sum += a.subs[i].balance;
    return static_cast<int32_t>(static_cast<uint32_t>(sum));
}

size_t category_count(const Account &a, uint8_t cat, const BankSubAccount **first = nullptr)
{
    size_t n = 0;
    for (size_t i = 0; i < a.nsubs; ++i) {
        if (a.subs[i].category == cat) {
            if (!n && first)
                *first = &a.subs[i];
            ++n;
        }
    }
    return n;
}

struct Cash {
    uintptr_t bank = 0;
    uint32_t team = 0;
    Account acct;
};

std::string cash_ready(Cash &c)
{
    c.bank = comp::get(comp::Id::CareerBank);
    if (!c.bank)
        return comp::entity() ? (ce::mode() == ce::Mode::DriverCareer ? "wrong_mode" : "not_ready: no CareerBank")
                              : "not_ready: no championship entity";
    if (!comp::view<CareerBankComponent>(c.bank))
        return "not_ready: bank unreadable";
    comp::Entrant p;
    if (!comp::player(p))
        return "no_player: " + std::to_string(comp::humans()) + " human entrants";
    c.team = p.team;
    if (!find_account(c.bank, c.team, c.acct))
        return "no_account";
    return {};
}

bool post_message(void *msg, std::string &why)
{
    swofty::career::Result r = swofty::career::post(msg);
    if (r == swofty::career::Result::ok)
        return true;
    why = r == swofty::career::Result::late ? "not_ready: message bus inactive"
                                            : std::string("not_ready: message bus ") + swofty::career::name(r);
    return false;
}

bool post_transaction(uint32_t team, int32_t amount, uint8_t type, std::string &why)
{
    BankTransactionMessage m{};
    m.vtable = vt_Career_BankTransactionMessage.get();
    m.account = static_cast<uint16_t>(((team & 0x3u) << 14) | ((team >> 16) & 0x3fffu));
    m.type = type;
    m.amount = amount;
    m.flow = kFlowImmediate;
    m.check_funds = 0;
    m.category = kCategoryTeamCash;
    return post_message(&m, why);
}

std::string cash_json(int64_t cash, uint32_t team)
{
    ce::Writer w;
    w.begin_object();
    w.key("cash");
    w.integer(cash);
    w.key("team");
    w.integer(team);
    w.end_object();
    return w.text();
}

ce::OpResult cash_apply(const ce::Json &args, bool absolute)
{
    int64_t v = 0;
    if (absolute ? !arg_int(args, "value", 0, kCashMax, v) : !arg_int(args, "amount", -kCashMax - 1, kCashMax, v))
        return fail(absolute ? "bad_args: value must be an integer 0..2147483647"
                             : "bad_args: amount must be an integer -2147483648..2147483647");
    if (!absolute && v == 0)
        return fail("bad_args: amount must not be 0");
    std::string ty = args["type"].str("reward");
    if (ty != "reward" && ty != "debug")
        return fail("bad_args: type must be \"reward\" or \"debug\"");
    uint8_t type = ty == "debug" ? kTypeDebug : kTypeMyTeamReward;

    Cash c;
    std::string err = cash_ready(c);
    if (!err.empty())
        return fail(err);
    const BankSubAccount *cash_sub = nullptr;
    size_t ncash = category_count(c.acct, kCategoryTeamCash, &cash_sub);
    if (ncash != 1)
        return fail("no_account: " + std::to_string(ncash) + " team cash sub-accounts");

    int64_t before = balance(c.acct);
    if (!expect_ok(args, before))
        return fail("expect_mismatch: live cash is " + std::to_string(before), cash_json(before, c.team));
    int64_t target = absolute ? v : std::clamp<int64_t>(before + v, 0, kCashMax);
    int64_t delta = target - before;
    ce::OpResult r;
    r.before = cash_json(before, c.team);
    if (delta == 0) {
        r.ok = true;
        r.after = r.before;
        ce::log("economy.cash.%s team=0x%x %lld -> %lld (no change)", absolute ? "set" : "add", c.team,
                (long long)before, (long long)before);
        return r;
    }
    std::string why;
    if (!post_transaction(c.team, static_cast<int32_t>(delta), type, why))
        return fail(why, r.before);

    Account now;
    int64_t after = find_account(c.bank, c.team, now) ? balance(now) : before;
    r.after = cash_json(after, c.team);
    ce::log("economy.cash.%s team=0x%x %lld -> %lld (BankAccountTransactionMessage %s %+lld)", absolute ? "set" : "add",
            c.team, (long long)before, (long long)after, kTxTypeNames[type], (long long)delta);
    if (after != target) {
        r.error = "game_refused: cash is " + std::to_string(after) + ", wanted " + std::to_string(target);
        return r;
    }
    r.ok = true;
    return r;
}

ce::OpResult op_cash_set(const ce::Json &args)
{
    return cash_apply(args, true);
}

ce::OpResult op_cash_add(const ce::Json &args)
{
    return cash_apply(args, false);
}

// ---------------------------------------------------------------------------
// Resource points (ResourcePointsComponent)

struct Rp {
    const ResourcePointsComponent *rp = nullptr;
    uint8_t mode = 0;
    uint32_t driver = 0;
    uint32_t key = 0;
};

uint32_t driver_key(const EntrantRecord &e)
{
    uint32_t k = 0;
    std::memcpy(&k, &e.id_lo, sizeof k);
    return k;
}

uint32_t team_of(const CareerEntity *entity, uint32_t driver)
{
    size_t n = 0;
    if (!entity || !comp::array(entity->entrants_begin, entity->entrants_end, comp::kMaxEntrants, n))
        return kNoKey;
    for (size_t i = 0; i < n; ++i)
        if (same_key(driver_key(entity->entrants_begin[i]), driver))
            return entity->entrants_begin[i].team_stat_id;
    return kNoKey;
}

std::string rp_ready(Rp &x)
{
    uintptr_t rp = comp::get(comp::Id::ResourcePoints);
    if (!rp)
        return comp::entity() ? "not_ready: no ResourcePoints" : "not_ready: no championship entity";
    x.rp = comp::view<ResourcePointsComponent>(rp);
    if (!x.rp || !comp::readable(reinterpret_cast<uintptr_t>(&x.rp->team_wallets), kWalletStoreBytes))
        return "not_ready: ResourcePoints unreadable";
    auto wallet_entity = reinterpret_cast<uintptr_t>(x.rp->wallet_entity);
    if (!comp::object_like(wallet_entity))
        return "not_ready: RP wallets not bound";
    x.mode = x.rp->wallet_mode;
    if (x.mode > 1)
        return "not_ready: RP wallet mode " + std::to_string(x.mode);
    comp::Entrant p;
    if (!comp::player(p))
        return "no_player: " + std::to_string(comp::humans()) + " human entrants";
    x.driver = p.driver;
    if (x.mode == 1) {
        x.key = team_of(comp::view<CareerEntity>(wallet_entity), p.driver);
        if ((x.key & 0xffff0000u) == kNoKey)
            x.key = p.team;
    } else {
        x.key = p.driver;
    }
    return {};
}

struct Wallet {
    int64_t value = 0;
    bool locked = false;
};

Wallet wallet(const Rp &x)
{
    uint64_t out = 0;
    Career_Get_RpWallet(const_cast<CareerEntity **>(&x.rp->wallet_entity), &out, x.driver);
    Wallet w;
    w.value = static_cast<int32_t>(static_cast<uint32_t>(out));
    w.locked = ((out >> 32) & 0xff) != 0;
    return w;
}

std::string rp_json(int64_t rp, const Rp &x)
{
    ce::Writer w;
    w.begin_object();
    w.key("rp");
    w.integer(rp);
    w.key("key");
    w.integer(x.key);
    w.key("per");
    w.str(x.mode == 1 ? "team" : "driver");
    w.end_object();
    return w.text();
}

ce::OpResult rp_apply(const ce::Json &args, bool absolute)
{
    int64_t v = 0;
    if (absolute ? !arg_int(args, "value", 0, kRpMax, v) : !arg_int(args, "amount", -kRpMax, kRpMax, v))
        return fail(absolute ? "bad_args: value must be an integer 0..999999"
                             : "bad_args: amount must be an integer -999999..999999");
    if (!absolute && v == 0)
        return fail("bad_args: amount must not be 0");

    Rp x;
    std::string err = rp_ready(x);
    if (!err.empty())
        return fail(err);
    Wallet before = wallet(x);
    if (!expect_ok(args, before.value))
        return fail("expect_mismatch: live RP is " + std::to_string(before.value), rp_json(before.value, x));
    if (before.locked)
        return fail("locked: the game flags this RP wallet as unlimited", rp_json(before.value, x));
    int64_t target = absolute ? v : std::clamp<int64_t>(before.value + v, 0, kRpMax);
    int64_t delta = target - before.value;
    ce::OpResult r;
    r.before = rp_json(before.value, x);
    if (delta == 0) {
        r.ok = true;
        r.after = r.before;
        ce::log("economy.rp.%s %s=0x%x %lld -> %lld (no change)", absolute ? "set" : "add",
                x.mode == 1 ? "team" : "driver", x.key, (long long)before.value, (long long)before.value);
        return r;
    }
    if (x.mode == 1)
        Career_Adjust_RpTeamWallet(const_cast<void **>(&x.rp->team_wallets), x.key, static_cast<int32_t>(delta));
    else
        Career_Adjust_RpDriverWallet(const_cast<void **>(&x.rp->driver_wallets), x.key, static_cast<int32_t>(delta));
    Wallet after = wallet(x);
    r.after = rp_json(after.value, x);
    ce::log("economy.rp.%s %s=0x%x driver=0x%x %lld -> %lld (%s %+lld)", absolute ? "set" : "add",
            x.mode == 1 ? "team" : "driver", x.key, x.driver, (long long)before.value, (long long)after.value,
            x.mode == 1 ? "Career_Adjust_RpTeamWallet" : "Career_Adjust_RpDriverWallet", (long long)delta);
    if (after.value != target) {
        r.error = "game_refused: RP is " + std::to_string(after.value) + ", wanted " + std::to_string(target);
        return r;
    }
    r.ok = true;
    return r;
}

ce::OpResult op_rp_set(const ce::Json &args)
{
    return rp_apply(args, true);
}

ce::OpResult op_rp_add(const ce::Json &args)
{
    return rp_apply(args, false);
}

// ---------------------------------------------------------------------------
// Renown (MyTeamRFComponent, Career_Get_TeamRenownData, vt_Career_RenownAwardedMessage)

constexpr size_t kMaxTiers = 64;
constexpr double kRenownMax = 100000.0;
constexpr size_t kCtrlFreeSlot = 1;

struct Renown {
    bool ok = false;
    uint32_t team = 0;
    double points = 0;
    int level = 0;
    std::vector<uint32_t> tiers;
};

void release_weak(uintptr_t cb)
{
    auto *ctrl = comp::view<NeSharedCtrl>(cb);
    if (!ctrl)
        return;
    auto *weak = const_cast<volatile int32_t *>(&ctrl->weak);
    if (__atomic_sub_fetch(weak, 1, __ATOMIC_ACQ_REL) == 0) {
        uintptr_t vt = reinterpret_cast<uintptr_t>(ctrl->vtable);
        uintptr_t fn = comp::in_image(vt) ? rd<uintptr_t>(vt + kCtrlFreeSlot * sizeof(void *)) : 0;
        if (comp::in_image(fn))
            reinterpret_cast<void (*)(uintptr_t)>(fn)(cb);
    }
}

std::string renown_read(Renown &r)
{
    uintptr_t rf = comp::get(comp::Id::MyTeamRF);
    if (!rf)
        return comp::entity() ? (ce::mode() == ce::Mode::DriverCareer ? "wrong_mode" : "not_ready: no MyTeamRF")
                              : "not_ready: no championship entity";
    comp::Entrant p;
    if (!comp::player(p))
        return "no_player: " + std::to_string(comp::humans()) + " human entrants";
    r.team = p.team;
    uintptr_t weak[2] = {0, 0};
    Career_Get_TeamRenownData(reinterpret_cast<void *>(rf), weak, r.team);
    auto *obj = comp::view<TeamRenownData>(weak[0]);
    auto *ctrl = comp::view<NeSharedCtrl>(weak[1]);
    std::string err;
    if (!obj || !ctrl || ctrl->strong <= 0) {
        err = "no_account: no renown data for the team";
    } else {
        r.points = obj->points;
        auto *cfg = comp::view<RenownConfig>(obj->config);
        size_t n = 0;
        if (cfg && comp::array(cfg->tiers_begin, cfg->tiers_end, kMaxTiers, n)) {
            for (size_t i = 0; i < n; ++i) {
                uint32_t v = cfg->tiers_begin[i];
                r.tiers.push_back(v);
                if (r.points >= static_cast<double>(v))
                    ++r.level;
            }
        }
        r.ok = true;
    }
    release_weak(weak[1]);
    return err;
}

std::string renown_json(const Renown &r)
{
    ce::Writer w;
    w.begin_object();
    w.key("renown");
    w.num(r.points);
    w.key("level");
    w.integer(r.level);
    w.key("team");
    w.integer(r.team);
    w.end_object();
    return w.text();
}

ce::OpResult renown_apply(const ce::Json &args, bool absolute)
{
    const ce::Json &a = args[absolute ? "value" : "amount"];
    if (a.type != ce::Json::Num || (absolute ? (a.n < 0 || a.n > kRenownMax) : (a.n < -kRenownMax || a.n > kRenownMax)))
        return fail(absolute ? "bad_args: value must be a number 0..100000"
                             : "bad_args: amount must be a number -100000..100000");
    if (!absolute && a.n == 0)
        return fail("bad_args: amount must not be 0");
    Renown before;
    std::string err = renown_read(before);
    if (!err.empty())
        return fail(err);
    const ce::Json &ex = args["expect"];
    double exv = ex.type == ce::Json::Num ? ex.n : ex.type == ce::Json::Obj && ex["renown"].type == ce::Json::Num ? ex["renown"].n : -1;
    if (ex.type != ce::Json::Null && (exv < 0 || std::abs(exv - before.points) > 0.01))
        return fail("expect_mismatch: live renown is " + std::to_string(before.points), renown_json(before));
    double target = absolute ? a.n : std::clamp(before.points + a.n, 0.0, kRenownMax);
    double delta = target - before.points;
    ce::OpResult r;
    r.before = renown_json(before);
    if (std::abs(delta) < 0.0005) {
        r.ok = true;
        r.after = r.before;
        return r;
    }
    std::string why;
    RenownAwardedMessage m{};
    m.vtable = vt_Career_RenownAwardedMessage.get();
    m.team = before.team;
    m.amount = static_cast<float>(delta);
    if (!post_message(&m, why))
        return fail(why, r.before);
    Renown after;
    renown_read(after);
    r.after = renown_json(after);
    ce::log("economy.renown.%s team=0x%x %.2f (level %d) -> %.2f (level %d) (MessageRAwarded %+.2f)",
            absolute ? "set" : "add", before.team, before.points, before.level, after.points, after.level, delta);
    if (!after.ok || std::abs(after.points - target) > 0.05) {
        r.error = "game_refused: renown is " + std::to_string(after.points) + ", wanted " + std::to_string(target);
        return r;
    }
    r.ok = true;
    return r;
}

ce::OpResult op_renown_set(const ce::Json &args)
{
    return renown_apply(args, true);
}

ce::OpResult op_renown_add(const ce::Json &args)
{
    return renown_apply(args, false);
}

// ---------------------------------------------------------------------------
// Sponsors (CareerSponsorsComponent, vt_Career_SponsorLoyaltyXpMessage). SponsorTeamRecord per
// team, SponsorEntry per sponsor with SponsorLoyaltyLevel thresholds. The loyalty handler
// (0x141106870) picks the first sponsor with slot == 0 and active != 0, as sponsor_primary() does.

constexpr size_t kMaxSponsors = 64;
constexpr size_t kMaxLevels = 64;
constexpr size_t kMaxSponsorTeams = 256;
constexpr int kSpReasons = 6;

const char *const kSpReasonNames[kSpReasons] = {"CompletedRaceWeekendPrimary", "CompletedRaceWeekendSecondary",
                                                "SeasonalGoal", "FacilityRequest", "DepartmentEventOutcome",
                                                "ActivityOutcome"};

struct Sponsor {
    const SponsorEntry *at = nullptr;
    uint16_t id = 0;
    std::string name;
    int32_t xp = 0;
    int level = 0;
    int32_t max_xp = 0;
    uint8_t slot = 0;
    bool active = false;
};

bool sponsor_list(uintptr_t sp_ptr, uint32_t team, std::vector<Sponsor> &out)
{
    auto *sp = comp::view<CareerSponsorsComponent>(sp_ptr);
    size_t nt = 0;
    if (!sp || !sp->teams_begin || !comp::array(sp->teams_begin, sp->teams_end, kMaxSponsorTeams, nt))
        return false;
    for (size_t t = 0; t < nt; ++t) {
        const SponsorTeamRecord &rec = sp->teams_begin[t];
        if (!same_key(rec.team, team))
            continue;
        if (rec.sponsors_begin == rec.sponsors_end)
            return true;
        size_t ns = 0;
        if (!rec.sponsors_begin || !comp::array(rec.sponsors_begin, rec.sponsors_end, kMaxSponsors, ns))
            return false;
        for (size_t i = 0; i < ns; ++i) {
            const SponsorEntry &e = rec.sponsors_begin[i];
            Sponsor x;
            x.at = &e;
            x.id = e.stat_id;
            x.xp = e.loyalty_xp;
            x.slot = e.slot;
            x.active = e.active != 0;
            swofty::mem::c_string(e.name, x.name);
            size_t nl = 0;
            if (e.levels_begin && comp::array(e.levels_begin, e.levels_end, kMaxLevels, nl)) {
                for (size_t l = 0; l < nl; ++l) {
                    int32_t th = e.levels_begin[l].xp_threshold;
                    if (th < 0)
                        continue;
                    x.max_xp = std::max(x.max_xp, th);
                    if (x.xp >= th)
                        ++x.level;
                }
            }
            out.push_back(x);
        }
        return true;
    }
    return false;
}

const Sponsor *sponsor_primary(const std::vector<Sponsor> &v)
{
    for (auto &s : v)
        if (s.slot == 0 && s.active)
            return &s;
    return nullptr;
}

struct Sponsors {
    uintptr_t sp = 0;
    uint32_t team = 0;
    std::vector<Sponsor> list;
};

std::string sponsors_read(Sponsors &s)
{
    s.sp = comp::get(comp::Id::CareerSponsors);
    if (!s.sp)
        return comp::entity() ? (ce::mode() == ce::Mode::DriverCareer ? "wrong_mode" : "not_ready: no CareerSponsors")
                              : "not_ready: no championship entity";
    comp::Entrant p;
    if (!comp::player(p))
        return "no_player: " + std::to_string(comp::humans()) + " human entrants";
    s.team = p.team;
    if (!sponsor_list(s.sp, s.team, s.list))
        return "no_account: no sponsor record for the team";
    return {};
}

std::string sponsor_json(const Sponsor *s, uint32_t team)
{
    ce::Writer w;
    w.begin_object();
    w.key("team");
    w.integer(team);
    w.key("sponsor");
    w.integer(s ? s->id : -1);
    w.key("name");
    w.str(s ? s->name : "");
    w.key("loyalty");
    w.integer(s ? s->xp : 0);
    w.key("level");
    w.integer(s ? s->level : 0);
    w.end_object();
    return w.text();
}

ce::OpResult op_sponsor_loyalty_add(const ce::Json &args)
{
    int64_t amount = 0;
    if (!arg_int(args, "amount", 1, 100000, amount))
        return fail("bad_args: amount must be an integer 1..100000");
    int64_t reason = 0;
    if (args["reason"].type != ce::Json::Null && !arg_int(args, "reason", 0, kSpReasons - 1, reason))
        return fail("bad_args: reason must be an integer 0..5");
    Sponsors before;
    std::string err = sponsors_read(before);
    if (!err.empty())
        return fail(err);
    const Sponsor *pb = sponsor_primary(before.list);
    if (!pb)
        return fail("no_account: no active primary sponsor");
    if (!expect_ok(args, pb->xp))
        return fail("expect_mismatch: live loyalty is " + std::to_string(pb->xp), sponsor_json(pb, before.team));
    ce::OpResult r;
    r.before = sponsor_json(pb, before.team);
    std::string why;
    SponsorLoyaltyXpMessage m{};
    m.vtable = vt_Career_SponsorLoyaltyXpMessage.get();
    m.team = before.team;
    m.amount = static_cast<int32_t>(amount);
    m.reason = static_cast<uint8_t>(reason);
    if (!post_message(&m, why))
        return fail(why, r.before);
    Sponsors after;
    sponsors_read(after);
    const Sponsor *pa = sponsor_primary(after.list);
    r.after = sponsor_json(pa, before.team);
    ce::log("economy.sponsor.loyalty.add team=0x%x sponsor=%u \"%s\" %d (level %d) -> %d (level %d) "
            "(PrimarySponsorLoyaltyXPGainedMessage +%lld %s)",
            before.team, pb->id, pb->name.c_str(), pb->xp, pb->level, pa ? pa->xp : -1, pa ? pa->level : -1,
            (long long)amount, kSpReasonNames[reason]);
    if (!pa || (pa->xp <= pb->xp && (pb->max_xp == 0 || pb->xp < pb->max_xp))) {
        r.error = "game_refused: loyalty did not move";
        return r;
    }
    r.ok = true;
    return r;
}

// ---------------------------------------------------------------------------
// Cost cap (CostCapComponent). Per-team data is a flat map (CostCapComponent.team_slots/team_ctrl)
// keyed by the u16 bank account id (kind << 14 | team stat id); a CostCapTeam holds a per-season
// flat map (season_slots/season_ctrl) of CostCapSeason {season_id, cached_expenses, cost_cap,
// cached_modifier} (serialiser 0x140c9d650: season_cost_cap_data). In the game's flat map the slot
// array ends where its control bytes start, one byte per slot, 0 = empty. The component's
// BankAccountValidTransactionMessage handler (0x140c9af30) moves cached_expenses: checked spending
// of types 11, 12, 28 adds to it, an unchecked eDamageRefund (6) takes amount x cached_modifier off
// it. The current season is the entity registry's vtable slot +0xf8, as that handler asks.

constexpr size_t kRegistrySeasonSlot = 0xf8 / sizeof(void *);
constexpr uint8_t kTypeDamageRefund = 6;
constexpr size_t kMaxMapSlots = 1u << 14;

template <class N, class F> bool map_each(N *const *slots, const uint8_t *ctrl, F &&f)
{
    auto lo = reinterpret_cast<uintptr_t>(slots);
    auto hi = reinterpret_cast<uintptr_t>(ctrl);
    if (!lo || hi < lo || (hi - lo) % sizeof(void *) || (hi - lo) / sizeof(void *) > kMaxMapSlots)
        return false;
    size_t n = (hi - lo) / sizeof(void *);
    if (n && (!comp::readable(lo, n * sizeof(void *)) || !comp::readable(hi, n)))
        return false;
    for (size_t i = 0; i < n; ++i) {
        if (!ctrl[i])
            continue;
        auto *node = comp::view<N>(slots[i]);
        if (node && f(*node))
            return true;
    }
    return true;
}

struct Season {
    int32_t id = 0;
    int32_t expenses = 0;
    int32_t cap = 0;
    float modifier = 0;
};

struct CostCap {
    uintptr_t cc = 0;
    uint32_t team = 0;
    int32_t current = -1;
    std::vector<Season> seasons;
    const Season *now() const
    {
        for (auto &s : seasons)
            if (s.id == current)
                return &s;
        return nullptr;
    }
};

int32_t current_season()
{
    auto *e = comp::view<CareerEntity>(comp::entity());
    if (!e)
        return -1;
    auto reg = reinterpret_cast<uintptr_t>(&e->component_registry);
    uintptr_t vt = reinterpret_cast<uintptr_t>(e->component_registry);
    uintptr_t fn = comp::in_image(vt) ? rd<uintptr_t>(vt + kRegistrySeasonSlot * sizeof(void *)) : 0;
    if (!comp::in_image(fn))
        return -1;
    return reinterpret_cast<int32_t (*)(uintptr_t)>(fn)(reg);
}

std::string costcap_read(CostCap &c)
{
    c.cc = comp::get(comp::Id::CostCap);
    if (!c.cc)
        return comp::entity() ? (ce::mode() == ce::Mode::DriverCareer ? "wrong_mode" : "not_ready: no CostCap")
                              : "not_ready: no championship entity";
    auto *cc = comp::view<CostCapComponent>(c.cc);
    if (!cc)
        return "not_ready: cost cap unreadable";
    comp::Entrant p;
    if (!comp::player(p))
        return "no_player: " + std::to_string(comp::humans()) + " human entrants";
    c.team = p.team;
    uint16_t account = static_cast<uint16_t>(((p.team & 0x3u) << 14) | ((p.team >> 16) & 0x3fffu));
    const CostCapTeam *team_node = nullptr;
    if (!map_each(cc->team_slots, cc->team_ctrl, [&](const CostCapTeam &node) {
            if (node.account != account)
                return false;
            team_node = &node;
            return true;
        }))
        return "not_ready: cost cap map unreadable";
    if (!team_node)
        return "no_account: no cost cap record for the team";
    map_each(team_node->season_slots, team_node->season_ctrl, [&](const CostCapSeason &node) {
        Season s;
        s.id = node.season_id;
        s.expenses = node.cached_expenses;
        s.cap = node.cost_cap;
        s.modifier = node.cached_modifier;
        if (s.id >= 0 && s.id < 1000 && std::isfinite(s.modifier) && s.modifier >= 0 && s.modifier <= 100)
            c.seasons.push_back(s);
        return c.seasons.size() >= 64;
    });
    std::sort(c.seasons.begin(), c.seasons.end(), [](const Season &a, const Season &b) { return a.id < b.id; });
    c.current = current_season();
    return {};
}

std::string costcap_json(const CostCap &c, int64_t cash)
{
    const Season *s = c.now();
    ce::Writer w;
    w.begin_object();
    w.key("season");
    w.integer(c.current);
    w.key("expenses");
    w.integer(s ? s->expenses : 0);
    w.key("cost_cap");
    w.integer(s ? s->cap : 0);
    w.key("cash");
    w.integer(cash);
    w.end_object();
    return w.text();
}

ce::OpResult op_costcap_refund(const ce::Json &args)
{
    int64_t amount = 0;
    if (!arg_int(args, "amount", 1, 100000000, amount))
        return fail("bad_args: amount must be an integer 1..100000000");
    CostCap before;
    std::string err = costcap_read(before);
    if (!err.empty())
        return fail(err);
    const Season *sb = before.now();
    if (!sb)
        return fail("no_account: no cost cap entry for season " + std::to_string(before.current));
    Cash c;
    err = cash_ready(c);
    if (!err.empty())
        return fail(err);
    int64_t cash_before = balance(c.acct);
    if (!expect_ok(args, sb->expenses))
        return fail("expect_mismatch: live expenses are " + std::to_string(sb->expenses), costcap_json(before, cash_before));
    if (cash_before + amount > kCashMax)
        return fail("bad_args: the refund would overflow the bank balance", costcap_json(before, cash_before));
    if (sb->modifier > 0) {
        int64_t most = static_cast<int64_t>(std::floor(static_cast<double>(sb->expenses) / sb->modifier));
        if (amount > most)
            return fail("bad_args: refund larger than this season's cost cap expenses (at most " +
                            std::to_string(std::max<int64_t>(most, 0)) + ")",
                        costcap_json(before, cash_before));
    }
    ce::OpResult r;
    r.before = costcap_json(before, cash_before);
    std::string why;
    if (!post_transaction(c.team, static_cast<int32_t>(amount), kTypeDamageRefund, why))
        return fail(why, r.before);
    CostCap after;
    costcap_read(after);
    Account now;
    int64_t cash_after = find_account(c.bank, c.team, now) ? balance(now) : cash_before;
    r.after = costcap_json(after, cash_after);
    const Season *sa = after.now();
    ce::log("economy.costcap.refund team=0x%x season=%d expenses %d -> %d, cash %lld -> %lld "
            "(BankAccountTransactionMessage eDamageRefund +%lld)",
            c.team, before.current, sb->expenses, sa ? sa->expenses : -1, (long long)cash_before,
            (long long)cash_after, (long long)amount);
    if (!sa || sa->expenses >= sb->expenses) {
        r.error = "game_refused: cost cap expenses did not move";
        return r;
    }
    r.ok = true;
    return r;
}

// ---------------------------------------------------------------------------
// Snapshot section `economy`

void section_renown(ce::Writer &w)
{
    Renown r;
    if (!renown_read(r).empty() || !r.ok) {
        w.null();
        return;
    }
    w.begin_object();
    w.key("points");
    w.num(r.points);
    w.key("level");
    w.integer(r.level);
    w.key("tiers");
    w.begin_array();
    for (uint32_t t : r.tiers)
        w.integer(t);
    w.end_array();
    w.end_object();
}

void section_sponsors(ce::Writer &w)
{
    Sponsors s;
    if (!sponsors_read(s).empty()) {
        w.null();
        return;
    }
    const Sponsor *prim = sponsor_primary(s.list);
    w.begin_object();
    w.key("primary");
    w.integer(prim ? prim->id : -1);
    w.key("list");
    w.begin_array();
    for (auto &x : s.list) {
        w.begin_object();
        w.key("id");
        w.integer(x.id);
        w.key("name");
        w.str(x.name);
        w.key("active");
        w.boolean(x.active);
        w.key("slot");
        w.integer(x.slot);
        w.key("loyalty");
        w.integer(x.xp);
        w.key("level");
        w.integer(x.level);
        w.key("max_loyalty");
        w.integer(x.max_xp);
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

void section_costcap(ce::Writer &w)
{
    CostCap c;
    if (!costcap_read(c).empty()) {
        w.null();
        return;
    }
    w.begin_object();
    w.key("current_season");
    w.integer(c.current);
    w.key("seasons");
    w.begin_array();
    for (auto &s : c.seasons) {
        w.begin_object();
        w.key("season");
        w.integer(s.id);
        w.key("expenses");
        w.integer(s.expenses);
        w.key("cost_cap");
        w.integer(s.cap);
        w.key("modifier");
        w.num(s.modifier);
        w.end_object();
    }
    w.end_array();
    w.end_object();
}

void section_cash(ce::Writer &w)
{
    Cash c;
    if (!cash_ready(c).empty()) {
        w.null();
        return;
    }
    w.begin_object();
    w.key("balance");
    w.integer(balance(c.acct));
    w.key("team");
    w.integer(c.team);
    w.key("accounts");
    w.begin_array();
    const BankSubAccount *cash_sub = nullptr;
    for (size_t i = 0; i < c.acct.nsubs; ++i) {
        const BankSubAccount &s = c.acct.subs[i];
        if (s.category == kCategoryTeamCash && !cash_sub)
            cash_sub = &s;
        w.begin_object();
        w.key("category");
        w.integer(s.category);
        w.key("balance");
        w.integer(s.balance);
        w.key("min");
        w.integer(s.minimum);
        w.end_object();
    }
    w.end_array();
    w.key("history");
    w.begin_array();
    auto hb = reinterpret_cast<uintptr_t>(cash_sub ? cash_sub->history_begin : nullptr);
    auto he = reinterpret_cast<uintptr_t>(cash_sub ? cash_sub->history_end : nullptr);
    if (hb && he > hb && (he - hb) % sizeof(BankTransaction) == 0 && (he - hb) / sizeof(BankTransaction) <= kMaxHistory) {
        size_t n = (he - hb) / sizeof(BankTransaction);
        size_t from = n > kHistoryShown ? n - kHistoryShown : 0;
        const BankTransaction *tx = cash_sub->history_begin;
        if (comp::readable(reinterpret_cast<uintptr_t>(tx + from), (n - from) * sizeof(BankTransaction))) {
            for (size_t i = from; i < n; ++i) {
                uint8_t type = tx[i].type;
                w.begin_object();
                w.key("type");
                w.integer(type);
                w.key("type_name");
                w.str(type < sizeof kTxTypeNames / sizeof kTxTypeNames[0] ? kTxTypeNames[type] : "?");
                w.key("amount");
                w.integer(tx[i].amount);
                w.key("day");
                w.integer(tx[i].time / kMinutesPerDay);
                w.end_object();
            }
        }
    }
    w.end_array();
    w.end_object();
}

void section_rp(ce::Writer &w)
{
    Rp x;
    if (!rp_ready(x).empty()) {
        w.null();
        return;
    }
    Wallet v = wallet(x);
    w.begin_object();
    w.key("wallet");
    w.integer(v.value);
    w.key("per");
    w.str(x.mode == 1 ? "team" : "driver");
    w.key("key");
    w.integer(x.key);
    w.key("unlimited");
    w.boolean(v.locked);
    w.end_object();
}

void section_economy(ce::Writer &w)
{
    w.begin_object();
    w.key("available");
    w.boolean(comp::entity() != 0);
    w.key("mode");
    w.str(mode_name());
    w.key("player");
    comp::Entrant p;
    if (comp::player(p)) {
        w.begin_object();
        w.key("driver");
        w.integer(p.driver);
        w.key("team");
        w.integer(p.team);
        w.end_object();
    } else {
        w.null();
    }
    w.key("cash");
    section_cash(w);
    w.key("rp");
    section_rp(w);
    w.key("cost_cap");
    section_costcap(w);
    w.key("sponsors");
    section_sponsors(w);
    w.key("renown");
    section_renown(w);
    w.end_object();
}

} // namespace

CE_OP("economy.cash.set", op_cash_set);
CE_OP("economy.cash.add", op_cash_add);
CE_OP("economy.rp.set", op_rp_set);
CE_OP("economy.rp.add", op_rp_add);
CE_OP("economy.renown.set", op_renown_set);
CE_OP("economy.renown.add", op_renown_add);
CE_OP("economy.sponsor.loyalty.add", op_sponsor_loyalty_add);
CE_OP("economy.costcap.refund", op_costcap_refund);
CE_SECTION("economy", section_economy);
