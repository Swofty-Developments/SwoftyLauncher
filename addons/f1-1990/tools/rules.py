#!/usr/bin/env python3
"""Writes db/rules.json and db/rules_text.json: the f1_1990 setup mode and its rules.

usage: rules.py [stock gamedatabases.2025.bin]
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ADDON = HERE.parent
sys.path.insert(0, str(ADDON.parent.parent / "tools"))
sys.path.insert(0, str(HERE))

import gamedb  # noqa: E402
from career import CAREER_YEARS, TRACK_DATES, TRACK_NAMES, race_sunday  # noqa: E402

STOCK_DB = "/mnt/work/.f1-1990/data-c26/fakegame/game_package/gamedatabases/gamedatabases.2025.bin"
FIELDS = json.loads((HERE / "rules_fields.json").read_text())

INT, STRING, ENUM, ID, BOOL, LIST, SUBSET = 0, 2, 3, 4, 5, 13, 18

NAME = "f1_1990"
MODE_STAT_ID = "16"
DATASET_STAT_ID = "20"
CLASS_STAT_ID = "12"
VISUAL_STAT_IDS = {"Supersoft_f1_1990": "28", "Ultrasoft_f1_1990": "29", "Soft_f1_1990": "30"}
ALLOCATION_STAT_ID = "14"
SLOT_COMPOUNDS = {"softer": "C4", "medium": "C5", "harder": "C3"}
MANDATORY_STAT_ID = "8"
STANDINGS_STAT_ID = "15"
TAG_NAME = "gamesetup_f1_1990"
STANDINGS_NAME = "f1_1990_race"
ALLOCATION_NAME = "f1_1990"
MANDATORY_NAME = "f1_1990_mandatory"
YEAR = "1990"

MODES = "game_setup.game_setup_modes"
MODE = f"{MODES}.game_setup_mode[_metadata_={NAME}]"
WEEKEND = f"{MODE}.weekend_options_0.weekend_structure[_metadata_=default]"
SESSIONS = f"{WEEKEND}.session_collection_0"
GROUPS = f"{WEEKEND}.session_group_collection_0"
DATASETS = "track_data.game_setup"
DATASET = f"{DATASETS}.TrackDataSet[_metadata_={NAME}]"
TAG_FOLDER = "options_system.context.tag_folder[_metadata_=game_setup_mode]"
TAG = f"{TAG_FOLDER}.game_setup_mode_tag[_metadata_={TAG_NAME}].restrictions_0"
PROBES = "tracks.ibl_probes.IblProbes_0"

SESSION_ROWS = [
    ("practice1", "practice_1", "race", ""),
    ("practice2", "practice_2", "race", ""),
    ("short_practice", "practice_s", "race", ""),
    ("short_sprint_shootout", "sprint_shootout_s", "race", ""),
    ("one_shot_sprint_shootout", "sprint_shootout_o", "race", ""),
    ("short_qualifying", "qualifying_s", "race", ""),
    ("one_shot_qualifying", "qualifying_o", "race", ""),
    ("race", "race", "not_set", STANDINGS_NAME),
]
LENGTH_GROUPINGS = [
    ("none", "none", ""),
    ("one_shot_qualifying", "one_shot", "{one_shot}"),
    ("short_qualifying", "short", "{short}"),
    ("full_qualifying", "full", "{short}"),
]
ALLOCATION_SESSIONS = [
    "Practice1", "Practice2", "Practice3", "Qualifying1", "Qualifying2", "Qualifying3",
    "Race", "Race2", "Race3", "SprintShootout1", "SprintShootout2", "SprintShootout3",
]
SOFT_SESSIONS = {"Practice1", "Practice2", "Race"}
QUALI_SESSIONS = {"SprintShootout1", "Qualifying1"}
SHOOTOUT_SESSIONS = {"SprintShootout1", "SprintShootout2", "SprintShootout3"}
NONE = "0,0,0,0,0"
QUALI_TYRE = "0,1,0,0,0"
POINTS = [9, 6, 4, 3, 2, 1]
HALF_POINTS = [4, 3, 2, 1, 1, 0]
POINT_SLOTS = 26
SUPERSOFT = {
    "material_context": "SuperSoft",
    "material_anim": "tyre_compound_key_super_soft",
    "audio_id": "SuperSoft",
    "icon_code": "\ue190",
    "colour": "-1762525",
    "texture_name": "SuperSoft_f1_1990",
    "display_name": "lng_tyre_compound_supersoft",
    "title": "lng_tyre_compound_supersoft_title",
    "desc": "lng_tyre_compound_supersoft_desc",
}
SOFT = {
    "material_context": "Soft",
    "material_anim": "tyre_compound_key_soft",
    "audio_id": "Soft",
    "icon_code": "\ue192",
    "colour": "-77565",
    "texture_name": "Soft_f1_1990",
    "display_name": "lng_tyre_compound_soft",
    "title": "lng_tyre_compound_soft_title",
    "desc": "lng_tyre_compound_soft_desc",
}
ULTRASOFT = {
    "material_context": "UltraSoft",
    "material_anim": "tyre_compound_key_ultra_soft",
    "audio_id": "UltraSoft",
    "icon_code": "\ue191",
    "colour": "-6860657",
    "texture_name": "UltraSoft_f1_1990",
    "display_name": "lng_tyre_compound_ultrasoft",
    "title": "lng_tyre_compound_ultrasoft_title",
    "desc": "lng_tyre_compound_ultrasoft_desc",
}
VISUALS = {"Supersoft_f1_1990": SUPERSOFT, "Ultrasoft_f1_1990": ULTRASOFT, "Soft_f1_1990": SOFT}

PIT_CREW = "pit_crew_training.probabilities"
PIT_CREW_SET = "probability_set"
PIT_CREW_NAME = "f1_1990"
PIT_CREW_FIELDS = ["max_training_level_percent", "min_routine_pit_stop_time", "max_routine_pit_stop_time",
                   "min_chance_mistake_percent", "max_chance_mistake_percent", "estimated_front_wing_change_time"]
PIT_CREW_BANDS = [
    (20, 10.5, 12.5, 21, 30),
    (40, 9.5, 11.5, 11, 20),
    (60, 8.5, 10.5, 6, 10),
    (80, 7.5, 9.5, 1, 5),
    (100, 7.0, 8.5, 0, 0),
]
NOSE_CHANGE_TIME = 18.0
PIT_TIMES = "pit_lane.pit_times"
PIT_TIMES_ROW = "team_pit_times"
TEAM_PIT_TIMES = {
    "mclaren_1990": (7.0, 8.6),
    "ferrari_1990": (7.1, 8.8),
    "williams_1990": (7.2, 9.0),
    "benetton_1990": (7.5, 9.4),
    "tyrrell_1990": (8.2, 10.2),
    "lotus_1990": (8.4, 10.5),
    "leytonhouse_1990": (8.5, 10.6),
    "larrousse_1990": (8.6, 10.8),
    "ligier_1990": (8.8, 11.0),
    "minardi_1990": (9.2, 11.6),
    "brabham_1990": (9.3, 11.8),
    "dallara_1990": (9.4, 12.0),
    "arrows_1990": (9.5, 12.2),
}
UPGRADED_GAIN = (0.5, 0.6)
PIT_LOSS_EXTRA = 7.0


class Stock:
    def __init__(self, path):
        self.recs = {r.key: r for r in gamedb.read(Path(path).read_bytes())}

    def get(self, key):
        r = self.recs.get(gamedb.fnv1a64(key))
        return None if r is None else (r.type, r.value.decode("utf-8"))

    def text(self, key):
        found = self.get(key)
        if found is None:
            raise SystemExit(f"stock database has no {key}")
        return found[1]

    def row(self, table, label):
        i = 0
        while (found := self.get(f"{table}_{i}._metadata_")) is not None:
            if found[1] == label:
                return f"{table}_{i}"
            i += 1
        raise SystemExit(f"stock database has no {table} row {label}")


def set_(select, field, value, type_=None, when=None):
    op = {"op": "set", "select": select, "field": field, "value": str(value)}
    if type_ is not None:
        op["create"] = True
        op["type"] = type_
    if when:
        op["when"] = when
    return op


def add_row(table, source, values):
    return {"op": "add-row", "table": table, "from": source, "set": {k: str(v) for k, v in values.items()}}


def points(values):
    return ",".join(str(v) for v in values + [0] * (POINT_SLOTS - len(values)))


def presets(fields):
    allocation = {"_metadata_": ALLOCATION_NAME, "stat_id": ALLOCATION_STAT_ID}
    mandatory = {"_metadata_": MANDATORY_NAME, "stat_id": MANDATORY_STAT_ID}
    for session in ALLOCATION_SESSIONS:
        if session in SOFT_SESSIONS:
            allocation[session] = "{{soft_sets}},0,{{soft_sets}},0,0"
        elif session in QUALI_SESSIONS:
            allocation[session] = "0,{{quali_sets}},0,0,0"
        else:
            allocation[session] = NONE
        mandatory[session] = QUALI_TYRE if session in SHOOTOUT_SESSIONS else NONE
    fields["tyres.allocation_tyre_presets.TyresPerSessionPreset"] = FIELDS["allocation"]
    fields["tyres.mandatory_tyre_presets.TyresPerSessionPreset"] = FIELDS["mandatory"]
    return [
        add_row("tyres.allocation_tyre_presets",
                "tyres.allocation_tyre_presets.TyresPerSessionPreset[_metadata_=softer]", allocation),
        add_row("tyres.mandatory_tyre_presets",
                "tyres.mandatory_tyre_presets.TyresPerSessionPreset[_metadata_=TyresPerSessionPreset]", mandatory),
    ]


def compounds(fields):
    fields["tyres.visual_compounds.VisualCompound"] = FIELDS["visual"]
    fields["tyres.compound_classes.CompoundClass"] = FIELDS["compound_class"]
    ops = [
        add_row("tyres.visual_compounds", "tyres.visual_compounds.VisualCompound[_metadata_=Soft_f1_26]",
                {"_metadata_": name, "stat_id": stat_id, **VISUALS[name]})
        for name, stat_id in VISUAL_STAT_IDS.items()
    ]
    softer, medium, harder = VISUAL_STAT_IDS
    ops.append(add_row("tyres.compound_classes", "tyres.compound_classes.CompoundClass[_metadata_=f1_26]", {
        "_metadata_": NAME,
        "game_setup_modes": NAME,
        "weekend_formats": "default",
        "stat_id": CLASS_STAT_ID,
        "CompoundOptions_0.visual_compounds": softer,
        "CompoundOptions_1.visual_compounds": medium,
        "CompoundOptions_2.visual_compounds": harder,
    }))
    ops.append(set_(f"tyres.compound_classes.CompoundClass[_metadata_={NAME}]", "show_allocation_screen", "False",
                    when={"no_tyre_allocation": True}))
    return ops


def dataset(fields, stock):
    tracks = FIELDS["dataset_tracks"]
    row_fields = []
    values = {"_metadata_": NAME, "game_setup_modes": NAME, "parent_data_set": "f1_2026",
              "weekend_formats": "default", "stat_id": DATASET_STAT_ID}
    for i, _ in enumerate(tracks):
        row = f"TrackData_{i}"
        row_fields += [f"{row}._metadata_", f"{row}.track", f"{row}._TyreData_instancecount_"]
        row_fields += [f"{row}.TyreData_0.{f}" for f in FIELDS["tyre_data"]]
        for compound in ("softer", "medium", "harder"):
            values[f"{row}.TyreData_0.compound_{compound}"] = SLOT_COMPOUNDS[compound]
        for preset in ("softer_preset", "balanced_preset", "harder_preset"):
            values[f"{row}.TyreData_0.{preset}"] = ALLOCATION_NAME
        values[f"{row}.TyreData_0.mandatory_tyres"] = MANDATORY_NAME
    fields[f"{DATASETS}.TrackDataSet"] = FIELDS["dataset_top"] + row_fields
    ops = [add_row(DATASETS, f"{DATASETS}.TrackDataSet[_metadata_=f1_2026]", values)]

    stock_2026 = stock.row(f"{DATASETS}.TrackDataSet", "f1_2026")
    stock_base = stock.row(f"{DATASETS}.TrackDataSet", "f1")
    for i, track in enumerate(tracks):
        row = f"{DATASET}.TrackData[_metadata_={track}]"
        tyre = f"{row}.TyreData_0"
        ops += [
            set_(tyre, "_MandatoryPitstopsOverride_instancecount_", 1, INT),
            set_(f"{tyre}.MandatoryPitstopsOverride_0", "_metadata_", "MandatoryPitstopsOverride", STRING),
            set_(f"{tyre}.MandatoryPitstopsOverride_0", "required_pitstops", 0, INT),
            set_(f"{tyre}.MandatoryPitstopsOverride_0", "session", "race", ID),
            set_(row, "_TrackAuthenticWeekendData_instancecount_", 1, INT),
            set_(f"{row}.TrackAuthenticWeekendData_0", "_metadata_", "TrackAuthenticWeekendData", STRING),
            set_(f"{row}.TrackAuthenticWeekendData_0", "weekend_format", "default", ID),
        ]
        sources = [f"{stock_2026}.TrackData_{i}", stock.row(f"{stock_base}.TrackData", track)]
        if track in TRACK_DATES:
            day, month = TRACK_DATES[track]
            ops += subtable(stock, row, sources, "TrackGeneralData", FIELDS["general_data"],
                            {"weekend_day": day, "weekend_month": month, "weekend_year": YEAR})
            ops += sunday_dates(stock, row, sources, day, month)
        ops += pit_data(stock, row, sources)
        if track in TRACK_NAMES:
            long_name, short_name = TRACK_NAMES[track]
            ops += subtable(stock, row, sources, "TrackNameData", FIELDS["name_data"],
                            {"gp_long_name": long_name, "gp_long_name_multiline": long_name,
                             "gp_short_name": short_name})
    return ops


def pit_data(stock, row, sources):
    source = next(s for s in sources if stock.get(f"{s}._TrackPitData_instancecount_") is not None)
    values = {}
    for field in ("pitStopTime_Dry", "pitStopTime_Wet"):
        values[field] = f"{float(stock.text(f'{source}.TrackPitData_0.{field}')) + PIT_LOSS_EXTRA:g}"
    return subtable(stock, row, sources, "TrackPitData", FIELDS["pit_data"], values)


def pit_crew(fields):
    rows = [f"probability_{i}" for i in range(len(PIT_CREW_BANDS))]
    fields[f"{PIT_CREW}.{PIT_CREW_SET}"] = ["_metadata_", "_probability_instancecount_"] + [
        f"{r}.{f}" for r in rows for f in ["_metadata_"] + PIT_CREW_FIELDS]
    values = {"_metadata_": PIT_CREW_NAME, "_probability_instancecount_": len(rows)}
    for r, (level, low, high, least, most) in zip(rows, PIT_CREW_BANDS):
        band = [level, low, high, least, most, NOSE_CHANGE_TIME]
        values.update({f"{r}.{f}": f"{v:g}" for f, v in zip(PIT_CREW_FIELDS, band)})
    ops = [add_row(PIT_CREW, f"{PIT_CREW}.{PIT_CREW_SET}[_metadata_=f1]", values)]
    fields[f"{PIT_TIMES}.{PIT_TIMES_ROW}"] = ["_metadata_", "team", "min_pit_time", "max_pit_time",
                                             "min_pit_time_fully_upgraded", "max_pit_time_fully_upgraded"]
    for team, (low, high) in TEAM_PIT_TIMES.items():
        ops.append(add_row(PIT_TIMES, f"{PIT_TIMES}.{PIT_TIMES_ROW}[_metadata_=mercedes]", {
            "_metadata_": team,
            "team": team,
            "min_pit_time": f"{low:g}",
            "max_pit_time": f"{high:g}",
            "min_pit_time_fully_upgraded": f"{low - UPGRADED_GAIN[0]:g}",
            "max_pit_time_fully_upgraded": f"{high - UPGRADED_GAIN[1]:g}",
        }))
    return ops


def sunday_dates(stock, row, sources, day, month):
    source = next(s for s in sources if stock.get(f"{s}._TrackGeneralData_instancecount_") is not None)
    ops = []
    for year in CAREER_YEARS:
        sunday = race_sunday(year, month, day)
        for field, value in (("weekend_day", sunday.day), ("weekend_month", sunday.month)):
            type_, _ = stock.get(f"{source}.TrackGeneralData_0.{field}")
            ops.append(set_(f"{row}.TrackGeneralData_0", field, value, type_, when={"career_year": str(year)}))
    return ops


def subtable(stock, row, sources, name, fields, values):
    source = next(s for s in sources if stock.get(f"{s}._{name}_instancecount_") is not None)
    ops = [set_(row, f"_{name}_instancecount_", 1, INT)]
    for field in fields:
        type_, value = stock.get(f"{source}.{name}_0.{field}")
        ops.append(set_(f"{row}.{name}_0", field, values.get(field, value), type_))
    return ops


def drs(fields):
    fields["drs.drs_allowed_lap.drs_allowed_lap_group"] = FIELDS["drs"]
    return [add_row("drs.drs_allowed_lap", "drs.drs_allowed_lap.drs_allowed_lap_group[_metadata_=f1_2024]", {
        "_metadata_": NAME,
        "drs_allowed_lap_0.lap_number": 200,
        "drs_allowed_lap_1.lap_number": 200,
        "drs_allowed_lap_2.lap_number": 200,
    })]


def standings(fields):
    fields["game_setup.session_standings.session_standing_preset"] = FIELDS["standings"]
    values = {"_metadata_": STANDINGS_NAME, "stat_id": STANDINGS_STAT_ID, "points": points(POINTS),
              "fastest_lap_bonus": 0, "fastest_lap_rule": "None"}
    for k in range(3):
        values.update({f"reduced_points_data_{k}.points": points(HALF_POINTS),
                       f"reduced_points_data_{k}.fastest_lap_bonus": 0,
                       f"reduced_points_data_{k}.fastest_lap_rule": "None"})
    return [add_row("game_setup.session_standings",
                    "game_setup.session_standings.session_standing_preset[_metadata_=standard_race]", values)]


def option_tag(fields, stock):
    fields[f"{TAG_FOLDER}.game_setup_mode_tag"] = FIELDS["tag"]
    ops = [
        add_row(TAG_FOLDER, f"{TAG_FOLDER}.game_setup_mode_tag[_metadata_=gamesetup_f1]", {"_metadata_": TAG_NAME}),
        set_(f"{TAG}.value_subset[_metadata_=qualifying_length]", "values",
             "qualifying_length,short_esports,full_f2,full"),
        set_(f"{TAG}.value_subset[_metadata_=weekend_structure]", "values", "weekend_structure,authentic,sprint"),
        set_(f"{TAG}.value_subset_15", "_metadata_", "safety_car_frequency", STRING),
        set_(f"{TAG}.value_subset_15", "values", "safety_car_frequency,reduced,standard,increased", SUBSET),
        set_(f"{TAG}.value_subset_16", "_metadata_", "parc_ferme_rules", STRING),
        set_(f"{TAG}.value_subset_16", "values", "parc_ferme_rules,on", SUBSET),
        set_(TAG, "_value_subset_instancecount_", 17),
    ]
    for key in FIELDS["tag_lists"]:
        select, field = key.rsplit(".", 1)
        ops.append(set_(select, field, f"{stock.text(key)},{TAG_NAME}"))
    return ops


def mode(fields):
    fields[f"{MODES}.game_setup_mode"] = FIELDS["mode"]
    ops = [add_row(MODES, f"{MODES}.game_setup_mode[_metadata_=f1_26]", {
        "_metadata_": NAME,
        "stat_id": MODE_STAT_ID,
        "display_name": "lng_f1_1990",
        "options_tag": TAG_NAME,
        "has_tyre_warmers": "False",
        "drs_allowed_lap_group": NAME,
        "tyre_regulations.allow_strategy_mandatory_stop_check": "False",
        "pit_crew_training_probability_set": PIT_CREW_NAME,
        "pit_crew_training_probability_set_equal_performance": PIT_CREW_NAME,
    })]
    ops += [
        set_(MODE, "show_grid_cutscene", "False", when={"grid_cutscene": False}),
        set_(MODE, "grid_rundown_enabled", "False", when={"grid_rundown": False}),
    ]
    for i, (label, session_id, penalties, standing) in enumerate(SESSION_ROWS):
        row = f"{SESSIONS}.session_{i}"
        ops += [
            set_(row, "_metadata_", label),
            set_(row, "session_id", session_id),
            set_(row, "session_to_apply_penalties_to", penalties),
            set_(row, "standings", standing),
        ]
    ops.append(set_(SESSIONS, "_session_instancecount_", len(SESSION_ROWS)))
    practice = f"{GROUPS}.practice_session_group_0.practice_session_length_grouping[_metadata_=full_practice]"
    ops.append(set_(practice, "sessions", "practice_1,practice_2"))
    friday = f"{GROUPS}.qualifying_session_group_0"
    ops += [set_(friday, "_metadata_", "sprint_shootout"), set_(friday, "group", "SprintShootout")]
    for i, (label, available_in, sessions) in enumerate(LENGTH_GROUPINGS):
        row = f"{friday}.qualifying_session_length_grouping_{i}"
        ops += [
            set_(row, "_metadata_", label),
            set_(row, "available_in", available_in),
            set_(row, "sessions", sessions.format(one_shot="sprint_shootout_o", short="sprint_shootout_s")),
        ]
    saturday = f"{GROUPS}.qualifying_session_group_1"
    ops += [
        set_(saturday, "_metadata_", "qualifying_session_group", STRING),
        set_(saturday, "group", "Qualifying", ENUM),
        set_(saturday, "_qualifying_session_length_grouping_instancecount_", len(LENGTH_GROUPINGS), INT),
    ]
    for i, (label, available_in, sessions) in enumerate(LENGTH_GROUPINGS):
        row = f"{saturday}.qualifying_session_length_grouping_{i}"
        ops += [
            set_(row, "_metadata_", label, STRING),
            set_(row, "available_in", available_in, LIST),
            set_(row, "sessions", sessions.format(one_shot="qualifying_o", short="qualifying_s"), LIST),
        ]
    ops.append(set_(GROUPS, "_qualifying_session_group_instancecount_", 2))
    return ops


def appends(stock):
    return [
        set_(PROBES, "game_setup_modes", f"{stock.text(f'{PROBES}.game_setup_modes')},{NAME}"),
    ]


def main():
    stock = Stock(sys.argv[1] if len(sys.argv) > 1 else STOCK_DB)
    fields = {}
    ops = (drs(fields) + standings(fields) + presets(fields) + compounds(fields) + pit_crew(fields) + dataset(fields, stock)
           + option_tag(fields, stock) + mode(fields) + appends(stock))
    rules = {"schema_version": 1, "fields": fields, "ops": ops}
    (ADDON / "db" / "rules.json").write_text(json.dumps(rules, indent=1, ensure_ascii=False) + "\n")
    text = {"schema_version": 1, "ops": [{"op": "lang.set", "language": "*", "key": "lng_f1_1990", "value": "F1 1990"}]}
    (ADDON / "db" / "rules_text.json").write_text(json.dumps(text, indent=1) + "\n")


if __name__ == "__main__":
    main()
