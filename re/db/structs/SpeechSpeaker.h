// struct: SpeechSpeaker
// subsystem: audio
// size: ?
// notes: One speaker of the speech system, reached through SpeechSystem.speakers + 0x60 + index * 0x88. In game the race engineer is "engineer1", a driver's radio reply "drivers", the commentators "commentator1" and "commentator2_alt1".
struct SpeechSpeaker {
    /* 0x018 */ char *name;                                  // L2 decomp:re/decomp/140a30b60.c;xref:0x140a30c39 the speaker part of the play_speech_<speaker>_<type> event name
};
