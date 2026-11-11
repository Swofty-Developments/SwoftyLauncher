#pragma once
#include "live_mem.hpp"

namespace swofty_core::live {

struct Row {
    Group group;
    bool ready;
    const char *needs;
};

inline constexpr Row ROWS[G_COUNT] = {
    {G_CAR_SYSTEMS, false, "CarLive.throttle, CarLive.brake, CarLive.input_flags, CarChassis.g_lat, CarSteering.angle"},
    {G_ERS, false, "CarErs live fields"},
    {G_WORLD, false, "CarBodyState.position"},
    {G_AERO, false, "CarF1Aero.drs_request"},
    {G_PROGRESS, false, "VehicleProgressSystem entries"},
    {G_CLASS, false, "ResultsSystem entries"},
    {G_TIMING, false, "TimingSystem entries"},
    {G_PIT, false, "PitStateSystem records"},
    {G_PENALTIES, false, "PenaltyLog records, MisconductManager"},
    {G_TYRES, false, "TyreSystem per-vehicle block"},
    {G_FLAGS, false, "MarshalZoneMonitor, RedFlagSystem, sector table"},
    {G_CONTROL, false, "ControllerSystem sets"},
    {G_SPECTATE, false, "SpectatorDirector.ui_active"},
    {G_MODE, false, "GameModeContext label"},
    {G_BADGES, false, "EntrantRecord team word, team tables"},
    {G_TRACK, false, "g_Game_TrackLength, track world name chain"},
};

}
