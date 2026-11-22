// struct: StandingsPointsLevel
// subsystem: session
// size: 0x18 L2 decomp:re/decomp/142a2cd50.c;decomp:re/decomp/1413cdff0.c
struct StandingsPointsLevel {
    /* 0x000 */ uint8_t fastest_lap_rule;     // L1 0 None, 1 BestOfTop10, 2 BestAndTop10; the bonus goes to the fastest lap's driver when the rule is 1/2 and position < 10
    /* 0x004 */ int32_t fastest_lap_bonus;    // L1
    /* 0x008 */ void *points;                 // L1 the database column object of "points": its i32 values run from +0x08 to +0x10, points[position]
    /* 0x010 */ float threshold;              // L1 FLT_MAX for the full points, else percentage_laps_completed_less_than_or_equal_to
};
