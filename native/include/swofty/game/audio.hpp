#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

struct SpeechLine {
    uint8_t _pad_0x0[0x80];
    uint64_t speaker; // L2 experimental
    uint32_t duration_ms; // L2 experimental
    uint32_t id; // L2 experimental
    uint8_t _pad_0x90[0xa8];
    char *subtitle_stem; // L2 experimental
    uint8_t category; // L2 experimental
    uint8_t _pad_0x141[0x3];
    uint32_t state; // L2 experimental
};
static_assert(offsetof(SpeechLine, speaker) == 0x80);
static_assert(offsetof(SpeechLine, duration_ms) == 0x88);
static_assert(offsetof(SpeechLine, id) == 0x8c);
static_assert(offsetof(SpeechLine, subtitle_stem) == 0x138);
static_assert(offsetof(SpeechLine, category) == 0x140);
static_assert(offsetof(SpeechLine, state) == 0x144);

struct SpeechSpeaker {
    uint8_t _pad_0x0[0x18];
    char *name; // L2 experimental
};
static_assert(offsetof(SpeechSpeaker, name) == 0x18);

struct SpeechSystem {
    uint8_t _pad_0x0[0x728];
    uint8_t *speakers; // L2 experimental
    uint8_t _pad_0x730[0x9c];
    uint32_t speaker_count; // L2 experimental
    uint8_t _pad_0x7d0[0x3c0];
    double radio_delay; // L2 experimental
};
static_assert(offsetof(SpeechSystem, speakers) == 0x728);
static_assert(offsetof(SpeechSystem, speaker_count) == 0x7cc);
static_assert(offsetof(SpeechSystem, radio_delay) == 0xb90);

using Snd_Request_JobWorker_t = void(void *worker, uint32_t job_type, uint32_t count);
inline constexpr fn<"Snd_Request_JobWorker", Snd_Request_JobWorker_t> Snd_Request_JobWorker{}; // L2 experimental
inline constexpr data<"g_Snd_JobWorkerLock", void *> g_Snd_JobWorkerLock{}; // L2 experimental
inline constexpr data<"g_Snd_JobWorkerOverflow", uint8_t[1152]> g_Snd_JobWorkerOverflow{}; // L2 experimental
inline constexpr data<"g_Snd_JobWorkerPending", int32_t> g_Snd_JobWorkerPending{}; // L2 experimental

}
