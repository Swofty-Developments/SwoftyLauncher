// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Career Mode Editor: the contract between the bridge (bridge.cpp) and the feature modules.
// A module registers its edit ops and its snapshot section at static init; the bridge drains the
// launcher's inbox on the flow thread, runs ops only while ce::safe() is true (ops registered with
// CE_OP_ANYTIME run whenever ce::in_career() is true), and writes every section into snapshot.json.
// protocol/<area>.md documents each op and section; PROTOCOL.md indexes them.

#pragma once
#include <cstdint>
#include <string>
#include <vector>

#include "../../../../native/src/core/json.hpp"
#include "instance.hpp"

namespace ce {

using Json = json::Value;

enum class Mode { None, MyTeam, DriverCareer };

class Writer {
  public:
    void begin_object();
    void end_object();
    void begin_array();
    void end_array();
    void key(const char *k);
    void str(const std::string &v);
    void num(double v);
    void integer(int64_t v);
    void boolean(bool v);
    void null();
    const std::string &text() const { return out_; }

  private:
    void sep();
    std::string out_;
    std::vector<bool> first_;
    bool after_key_ = false;
};

struct OpResult {
    bool ok = false;
    std::string error;
    std::string before;
    std::string after;
};

using OpFn = OpResult (*)(const Json &args);
using SectionFn = void (*)(Writer &w);

void register_op(const char *name, OpFn fn);
void register_op_anytime(const char *name, OpFn fn);
void register_section(const char *name, SectionFn fn);

// Work a module does a slice at a time while the launcher page is open, one call per frame:
// 0 nothing to do, 1 more to do, 2 finished (the sections render again).
using IdleFn = int (*)();
void register_idle(IdleFn fn);

Mode mode();
bool in_career();
bool safe();
std::string career_id();
std::string career_renamed_to(const std::string &from);
std::string legacy_career_key();

void log(const char *fmt, ...);

// Registers the bridge's frame callback if the plugin's load could not. Cheap once it is in; the
// modules' game hooks call it so a refused registration recovers.
void ensure_frame();

// The core's profiler (swofty/perf.log), found through the core's exports; both are no-ops on a core
// without it. Timed counts a game hook's own time; GameCall marks the game's original running inside
// it, which is not counted.
uint32_t perf_probe(const char *what);

class Timed {
  public:
    explicit Timed(uint32_t probe);
    ~Timed();
    Timed(const Timed &) = delete;
    Timed &operator=(const Timed &) = delete;

  private:
    bool on_;
};

class GameCall {
  public:
    GameCall();
    ~GameCall();
    GameCall(const GameCall &) = delete;
    GameCall &operator=(const GameCall &) = delete;

  private:
    bool on_;
};

} // namespace ce

#define CE_CAT_(a, b) a##b
#define CE_CAT(a, b) CE_CAT_(a, b)
#define CE_OP(name, fn) static const bool CE_CAT(ce_op_, __LINE__) = (::ce::register_op(name, fn), true)
#define CE_OP_ANYTIME(name, fn) \
    static const bool CE_CAT(ce_op_any_, __LINE__) = (::ce::register_op_anytime(name, fn), true)
#define CE_IDLE(fn) static const bool CE_CAT(ce_idle_, __LINE__) = (::ce::register_idle(fn), true)
#define CE_SECTION(name, fn) static const bool CE_CAT(ce_section_, __LINE__) = (::ce::register_section(name, fn), true)
