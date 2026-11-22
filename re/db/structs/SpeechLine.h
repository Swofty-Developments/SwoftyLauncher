// struct: SpeechLine
// subsystem: audio
// size: ?
// notes: One voice line Game_SpeechSystem_PostLine posts. The caller fills the speaker, category and subtitle stem; PostLine sets the end time from duration_ms and posts the Wwise event with the speech system's callback object (speech +0x858), whose handlers set duration_ms to the real length a few frames later and move state. A line is a reused slot: id changes when the slot carries the next line.
struct SpeechLine {
    /* 0x080 */ uint64_t speaker;                            // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30c10 speaker index into SpeechSystem.speakers, bounded by SpeechSystem.speaker_count
    /* 0x088 */ uint32_t duration_ms;                        // L2 decomp:re/decomp/140a30b60.c;decomp:re/decomp/140ac11a0.c;xref:0x140a30b89,0x140ac11a0 length in ms: PostLine reads it for the end time (+0x98 = now + duration_ms / 1000); 0 at PostLine for a new line, the duration callback (thunk 0x140ac11a0) writes the real length
    /* 0x08c */ uint32_t id;                                 // L2 decomp:re/decomp/140a36730.c;xref:0x140a37cd0,0x140a38024,0x140a39e52 line id the speech update matches lines by; a new value when the slot is reused
    /* 0x138 */ char *subtitle_stem;                         // L2 decomp:re/decomp/140a36730.c;xref:0x140a397b9 subtitle key stem, copied into a 0x40-byte string by the speech update; the text key is lng_subtitle_<stem> ("eng_radiocheck_03")
    /* 0x140 */ uint8_t category;                            // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30be5,0x140a30c87 indexes the 4-entry event type table 0x146051dc0; 2 is team radio (takes SpeechSystem.radio_delay), 0 commentary
    /* 0x144 */ uint32_t state;                              // L2 decomp:re/decomp/140ac1160.c;decomp:re/decomp/140ac11a0.c;xref:0x140ac1160,0x140ac11a0 written by the Wwise callback handlers: 8 when the event ends (thunk 0x140ac1160), 2 or 4 from the duration callback unless it is 6 (playing)
};
