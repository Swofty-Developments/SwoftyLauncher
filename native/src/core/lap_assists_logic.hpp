#pragma once

#include <swofty/json.hpp>

#include <cstdint>
#include <cstdio>
#include <string>

// The player's driver assists as the game reports them for a Time Trial leaderboard entry
// (Game_GhostLapHandler_EvaluateDriverAssists 0x142f9fda0): bytes of the telemetry lap's summary block (TLap +0x278..+0x281,
// summary offsets 0x18..0x21). The packed value is exactly what the game puts in lbe.DriverAssists. The input device is the
// car's own classification (car +0x220: 0 pad, 1 wheel, 2 keyboard, 3 direct), the value the game writes as CarSetup.
namespace runtime::lap_assists {

constexpr size_t SUMMARY_BRK = 0x18, SUMMARY_ABS = 0x19, SUMMARY_TRC = 0x1a, SUMMARY_DRL = 0x1b, SUMMARY_GBX = 0x1c,
                 SUMMARY_ERS = 0x1d, SUMMARY_AUX = 0x21, SUMMARY_SIZE = 0x6c;
constexpr uint64_t CAR_INPUT = 0x220, CAR_PERFORMANCE = 0x12c;

struct Raw {
    bool ok = false;
    uint8_t brk = 0, abs = 0, trc = 0, drl = 0, gbx = 0, ers = 0, aux = 0;
};

// 0xff is the constructor default of an unwritten summary; anything above 32 is not an assist level.
inline bool from_summary(const uint8_t *s, Raw &out)
{
    out = Raw{};
    out.brk = s[SUMMARY_BRK];
    out.abs = s[SUMMARY_ABS];
    out.trc = s[SUMMARY_TRC];
    out.drl = s[SUMMARY_DRL];
    out.gbx = s[SUMMARY_GBX];
    out.ers = s[SUMMARY_ERS];
    out.aux = s[SUMMARY_AUX];
    for (uint8_t v : {out.brk, out.abs, out.trc, out.drl, out.gbx, out.ers, out.aux})
        if (v > 32)
            return false;
    out.ok = true;
    return true;
}

inline uint32_t packed(const Raw &r)
{
    return (r.abs != 0) | (r.brk != 0) << 1 | (r.drl != 0) << 2 | (r.gbx == 3) << 3 | (r.trc != 0) << 4 | (r.aux == 1) << 5 |
           (r.ers != 1) << 6;
}

// The more assisted of two readings of one lap (a setting changed mid-lap).
inline Raw merge(const Raw &a, const Raw &b)
{
    if (!a.ok)
        return b;
    if (!b.ok)
        return a;
    Raw m = a;
    m.brk = a.brk > b.brk ? a.brk : b.brk;
    m.abs = a.abs > b.abs ? a.abs : b.abs;
    m.trc = a.trc > b.trc ? a.trc : b.trc;
    m.drl = a.drl > b.drl ? a.drl : b.drl;
    m.gbx = (a.gbx == 3 || b.gbx == 3) ? 3 : (a.gbx < b.gbx ? a.gbx : b.gbx);
    m.ers = (a.ers != 1 || b.ers != 1) ? (a.ers != 1 ? a.ers : b.ers) : 1;
    m.aux = a.aux == 1 || b.aux == 1 ? 1 : a.aux;
    return m;
}

inline const char *device_name(int code)
{
    switch (code) {
    case 0: return "controller";
    case 1: return "wheel";
    case 2: return "keyboard";
    default: return "";
    }
}

inline swofty::json::Value to_json(const Raw &r, int device)
{
    using swofty::json::Value;
    Value o = Value::object({{"brk", int64_t(r.brk)}, {"abs", int64_t(r.abs)}, {"trc", int64_t(r.trc)}, {"drl", int64_t(r.drl)},
                             {"gbx", int64_t(r.gbx)}, {"ers", int64_t(r.ers)}, {"aux", int64_t(r.aux)}});
    if (device >= 0 && device_name(device)[0])
        o["input_device"] = std::string(device_name(device));
    return o;
}

inline std::string diag_text(const Raw &r, int device, int performance)
{
    char b[200];
    snprintf(b, sizeof b, "packed 0x%02x tc=%u abs=%u brk=%u line=%u gbx=%u ers=%u aux=%u input=%s performance=%d", packed(r),
             r.trc, r.abs, r.brk, r.drl, r.gbx, r.ers, r.aux, device_name(device)[0] ? device_name(device) : "unknown",
             performance);
    return b;
}

}
