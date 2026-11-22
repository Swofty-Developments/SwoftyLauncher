// struct: SpeechSystem
// subsystem: audio
// size: ?
// notes: The speech system Game_SpeechSystem_PostLine takes first. A line's speaker name is *(*(speakers + 0x60 + line.speaker * 0x88) + 0x18) when line.speaker < speaker_count, else empty; PostLine builds the Wwise event play_speech_<speaker>_<type> from it (SpeechLine.category picks the type).
struct SpeechSystem {
    /* 0x728 */ uint8_t *speakers;                           // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30c22 speaker table: one 0x88-byte slot per speaker from +0x60, each holding a SpeechSpeaker pointer
    /* 0x7cc */ uint32_t speaker_count;                      // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30c17 PostLine reads the speaker name only when line.speaker < speaker_count
    /* 0xb90 */ double radio_delay;                          // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30cb4 seconds PostLine adds to a radio line's end time (+0x98) when the line is category 2
};
