#!/usr/bin/env python3
"""Writes db/rnd.json, db/rnd-lang.json, data/rnd-strings.json and data/rnd-art.json: the 1990 R&D.

usage: rnd.py <stock gamedatabases.2025.bin> [--check]

The 1990 career reads its upgrades from `upgrades.1990` instead of `upgrades.2026`. The prefix is the
`upgrades_path` of the career's ComUpgrades component, which a 2026 Driver Career takes from the shared
component set `shared_driver_career_26_components`. The 1990 championship gets its own set,
`shared_driver_career_1990_components`, whose ComUpgrades, ComResourcePoints, ComAccolades and ComSchedule
point at the 1990 rows. Under `upgrades.1990` the engine supplier trees, delivery schedules, regulation
change schedules, R&D challenges and `general` (the R&D UI package and the dual-development scalars) are copies
of the 2026 ones; `trees.<token>` holds the 1990 trees.

The 1990 cars drive the 1990 engine rows `f1990_engine_<supplier>` (build_cars.py), so each copied engine supplier
tree links 1990 engines instead of its 2026 engine (ENGINE_TREES): `engines_linked_to` lists them and
`parent_engine` is the first.

A team finds its tree through `trees.<token>.teams_linked_to_0.links`, a list of vehicle labels
(Career_Read_TeamLinks 0x1435396e0 walks 12 tokens compiled into the exe and keeps the first whose list
names the team's vehicle). Each 1990 team has its own token except Lotus and Larrousse, the two
Lamborghini teams, which share one. The lists name both the A car and the B-spec car.

Each tree's performance ratings are the template's scaled by the 1990 pace of its teams (rating_scale); the
Vehicle Performance Comparison chart starts in that order.

No stock key changes: every op writes a new key, a key of a 1990 row another lane adds, or the row count
of `championships.shared_components`. --check exits 1 when the shipped files differ from a fresh build.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import career  # noqa: E402
from season import Season, slots  # noqa: E402

gamedb = career.gamedb
OUT_DB = os.path.join(ADDON, "db", "rnd.json")
OUT_LANG = os.path.join(ADDON, "db", "rnd-lang.json")
OUT_STRINGS = os.path.join(ADDON, "data", "rnd-strings.json")
OUT_ART = os.path.join(ADDON, "data", "rnd-art.json")

STOCK = "upgrades.2026"
PREFIX = "upgrades.1990"
TEMPLATE_TOKEN = "mclaren"
SHARED = "championships.shared_components"
SHARED_SOURCE = "shared_driver_career_26_components"
SHARED_1990 = "shared_driver_career_1990_components"
SHARED_LIST = f"shared_driver_career_components,shared_driver_career_offline_components,{SHARED_1990}"
COPIED = ["engine_suppliers.trees", "engine_suppliers.delivery_schedules", "regulation_changes_schedules",
          "rnd_challenges", "general"]
# Keys of the copied subtrees that every stock row has but the key catalogue does not name. `general` is read by
# the upgrades loader's vt+0x58 (0x143540790, `%s.general` table `ui_data`): texture_array_dictionary_path is the
# R&D UI package every card, icon and strip path starts with (#overrides.career.rnd_om_xm), term_x_mode the X-mode
# wording flag; global_tweakables holds the dual-development scalars. Without it the 1990 package is empty and no
# R&D art resolves.
UNCATALOGUED = {
    "engine_suppliers.trees": ["engine_supplier_upgrades_N.engine.unused"],
    "general": ["_global_tweakables_instancecount_", "global_tweakables_N._metadata_",
                "global_tweakables_N.dual_development_cost_scalar", "global_tweakables_N.dual_development_time_scalar",
                "_ui_data_instancecount_", "ui_data_N._metadata_", "ui_data_N.term_x_mode",
                "ui_data_N.texture_array_dictionary_path"],
}

STR, INT, LIST = 2, 0, 13

ENGINE_TREES = {"ferrari": ("ferrari",), "honda": ("honda",), "mercedes": ("renault",),
                "redbull_ford": ("ford_hb", "ford_dfr"), "audi": ("lamborghini", "judd")}
ENGINE_LINK_FIELDS = ("engines_linked_to", "parent_engine")

# token -> (teams, engine supplier, works engine, tyre supplier)
TOKENS = {
    "mclaren": (["mclaren"], "honda", True, "goodyear"),
    "ferrari": (["ferrari"], "ferrari", True, "goodyear"),
    "williams": (["williams"], "renault", True, "goodyear"),
    "renault": (["benetton"], "ford_hb", True, "goodyear"),
    "mercedes": (["tyrrell"], "ford_dfr", False, "pirelli"),
    "redbull": (["lotus", "larrousse"], "lamborghini", False, "goodyear"),
    "force_india": (["leytonhouse"], "judd", False, "goodyear"),
    "sauber": (["ligier"], "ford_dfr", False, "goodyear"),
    "tororosso": (["minardi"], "ford_dfr", False, "pirelli"),
    "gm_cadillac": (["brabham"], "judd", False, "pirelli"),
    "haas": (["arrows"], "ford_dfr", False, "goodyear"),
    "user_defined": (["dallara"], "ford_dfr", False, "pirelli"),
}
PACK_TEAM = {"leytonhouse": "march", "ligier": "ligier_prost"}

# Engine Power at full progress, per supplier: bhp on the car (metadata bhp_adjustment_fully_upgraded).
SUPPLIER_BHP = {"honda": 75, "ferrari": 72, "renault": 66, "ford_hb": 54, "lamborghini": 42, "judd": 33,
                "ford_dfr": 24}

SIZES = {
    "Minor": {"cost": 400, "research": 7, "develop": 7, "weighting": "1.0"},
    "Major": {"cost": 800, "research": 14, "develop": 10, "weighting": "2"},
    "Ultimate": {"cost": 1400, "research": 24, "develop": 14, "weighting": "3"},
}
COST_TO_DEVELOP = 1000

FD, RD, DRAG = "Front Downforce", "Rear Downforce", "Drag"
CWR, CWD, TW, BR = "Chassis Weight Reduction", "Chassis Weight Redistribution", "Tyre Wear", "Brakes"
EP, FC = "Engine Power", "Fuel Consumption"
GB, ICE, CE = "Gearbox Durability", "Internal Combustion Engine Durability", "Control Electronics Durability"
DEPT = {FD: "Aerodynamics", RD: "Aerodynamics", DRAG: "Aerodynamics", CWR: "Chassis", CWD: "Chassis",
        TW: "Chassis", BR: "Chassis", EP: "Engine", FC: "Engine", GB: "Durability", ICE: "Durability",
        CE: "Durability"}


class U:
    def __init__(self, name, type_, size, xy, links=(), only=None, cost=None, days=None, quali=False,
                 art=None, title="", text=""):
        self.name, self.type, self.size, self.xy, self.links = name, type_, size, xy, list(links)
        self.only, self.cost, self.days, self.quali, self.art = only, cost, days, quali, art
        self.title, self.text = title, text

    @property
    def label(self):
        return f"f1990_{self.name}"


# `only`: None = every tree, "works" = works engine trees only. Links name children (the stock convention).
UPGRADES = [
    U("raised_nose", FD, "Minor", (-1, -3), ["anhedral_front_wing"], art="om_xm_front_nose_downforce",
      title="Raised Nose",
      text="Lifting the nose feeds clean air under the car to the flat bottom and the diffuser."),
    U("diffuser_profile", RD, "Minor", (1, -3), ["multi_element_rear_wing"],
      art="om_xm_diffuser_refined_strake_layout", title="Diffuser Profile",
      text="A steeper diffuser exit that pulls more air from under the flat bottom."),
    U("anhedral_front_wing", FD, "Major", (-1, -4), ["sidepod_cooling"], art="om_xm_front_wing_main_plane",
      title="Anhedral Front Wing",
      text="A front wing slung under the raised nose. More front grip, a little more drag."),
    U("multi_element_rear_wing", RD, "Major", (1, -4), ["sidepod_cooling"],
      art="om_xm_rear_wing_secondary_elements", title="Multi-Element Rear Wing",
      text="A third rear-wing element for the slow circuits. Downforce at the cost of straight-line speed."),
    U("sidepod_cooling", DRAG, "Minor", (0, -5), ["flat_bottom_package"], art="om_xm_side_pod_profile_drag",
      title="Sidepods and Cooling Exits",
      text="Tighter sidepods and smaller radiator exits cut drag; the engine runs hotter for it."),
    U("flat_bottom_package", RD, "Ultimate", (0, -6), art="om_xm_diffuser_redesigned_geometry",
      title="Flat-Bottom Aero Package",
      text="A new undertray, diffuser and rear wing developed together in the wind tunnel."),

    U("carbon_tub", CWR, "Minor", (3, 0), ["weight_programme"], art="om_xm_monocoque_structure",
      title="Carbon-Fibre Tub",
      text="A stiffer, lighter monocoque lay-up from the autoclave."),
    U("weight_programme", CWR, "Major", (4, 0), ["pushrod_suspension", "bspec_package", "carbon_brakes"],
      art="om_xm_cockpit_padding", title="Weight-Saving Programme",
      text="Titanium fasteners, lighter ancillaries and ballast moved low, towards the 500 kg limit."),
    U("pushrod_suspension", TW, "Minor", (4, 1), ["active_suspension"], art="om_xm_suspension_geometry",
      title="Pushrod Suspension Geometry",
      text="Revised pushrod and rocker geometry that keeps the tyres flatter on the road."),
    U("bspec_package", CWR, "Ultimate", (4, -1), cost=1600, days=(24, 14), art="om_xm_engine_cover_reduction",
      title="B-Spec Car",
      text="The mid-season evolution: a new chassis built on everything learnt so far. The team races the "
           "B-spec car once it is fitted."),
    U("carbon_brakes", BR, "Minor", (5, 0), art="om_xm_pad_material", title="Carbon Brake Package",
      text="Carbon discs and pads with new cooling ducts."),
    U("active_suspension", CWD, "Ultimate", (5, 1), cost=1800, days=(28, 14), art="om_xm_heave_dampers",
      title="Active Suspension",
      text="Computer-controlled hydraulic suspension that holds the ride height. A big, late project."),

    U("camshaft_profiles", EP, "Minor", (-4, 0), ["valve_train"], art="om_xm_valve_timing",
      title="Camshaft Profiles", text="New cam profiles for more power at the top of the rev range."),
    U("valve_train", EP, "Major", (-5, 0), ["fuel_mapping", "quali_rev_limit", "evolution_spec"],
      art="om_xm_crankshaft", title="High-Rev Valve Train",
      text="A lighter valve train that lets the engine rev higher."),
    U("fuel_mapping", FC, "Minor", (-5, -1), ["fuel_package"], art="om_xm_fuel_flow_regulator",
      title="Lean Fuel Mapping",
      text="With refuelling banned, every litre counts: a leaner mixture for the opening laps."),
    U("fuel_package", FC, "Major", (-6, -1), art="om_xm_fuel_injection_system_efficiency",
      title="Low-Consumption Package",
      text="Combustion chamber and injection work that lets the car start with less fuel."),
    U("evolution_spec", EP, "Ultimate", (-6, 0), only="works", cost=1600, art="om_xm_air_intake_manifold",
      title="Evolution-Spec Engine",
      text="The works engine's next specification, only for the factory team."),
    U("quali_rev_limit", EP, "Minor", (-5, 1), ["quali_engine"], quali=True, art="om_xm_ignition_system",
      title="Qualifying Engine Mode",
      text="A higher rev limit and a richer mixture, used only in qualifying."),
    U("quali_engine", EP, "Major", (-6, 1), only="works", quali=True, cost=900, art="om_xm_lightweight_pistons",
      title="Qualifying Engine",
      text="A short-life engine built for one qualifying session. Qualifying only."),

    U("gearbox_internals", GB, "Minor", (0, 4), ["engine_rebuild", "electrics", "gearbox_casing"],
      art="om_xm_gearbox_wear_1", title="Gearbox Internals",
      text="Stronger gears and selector forks for the manual gearbox."),
    U("engine_rebuild", ICE, "Minor", (1, 4), ["cooling_system"], art="om_xm_ice_wear_1",
      title="Engine Rebuild Standards",
      text="Tighter tolerances at every rebuild between races."),
    U("electrics", CE, "Minor", (-1, 4), art="om_xm_control_electronics_wear_1", title="Wiring and Electrics",
      text="A new loom and sealed connectors. Fewer electrical retirements."),
    U("cooling_system", ICE, "Major", (2, 4), art="om_xm_ice_wear_2", title="Radiator and Oil Cooling",
      text="Bigger oil coolers and a revised radiator core for the hot races."),
    U("gearbox_casing", GB, "Major", (0, 5), ["semi_auto_gearbox"], art="om_xm_gearbox_wear_2",
      title="Gearbox Casing and Oil System",
      text="A stiffer casing and a new oil system for the gearbox."),
    U("semi_auto_gearbox", GB, "Ultimate", (0, 6), cost=1600, days=(28, 14), art="om_xm_gearbox_wear_3",
      title="Semi-Automatic Gearbox",
      text="A paddle-shift gearbox project. Faster, safer shifts once it is reliable."),
]
TEAM_TEXT = {
    ("ferrari", "semi_auto_gearbox"): ("Paddle-Shift Reliability",
                                       "Ferrari raced the first paddle-shift gearbox; this project makes it last."),
}
ROOTS = ["raised_nose", "diffuser_profile", "carbon_tub", "gearbox_internals", "camshaft_profiles"]
WAYPOINTS = [
    ("diffuser_profile", "0,0,1,1,0,0,0,0,0,0", "0,-2.5,-2.5,-3,0,0,0,0,0,0"),
    ("raised_nose", "0,0,-1,-1,0,0,0,0,0,0", "0,-2.5,-2.5,-3,0,0,0,0,0,0"),
]
DEPARTMENT_FILES = {"Aerodynamics": "aerodynamics", "Chassis": "chassis", "Engine": "engine",
                    "Durability": "durability"}

ROW_FIELDS = ["_metadata_", "upgrade_name", "upgrade_type", "upgrade_size", "upgrade_department", "cost",
              "cost_to_develop", "weighting", "unlock_at_facility_level", "supplier_delivery_date_index", "links",
              "fitting_times.num_days_to_research", "fitting_times.num_days_to_develop", "grid_coordinates.x",
              "grid_coordinates.y"]
META_FIELDS = ["_metadata_", "affect_on_vehicle", "affect_on_vehicle_unit_type", "bhp_adjustment_fully_upgraded",
               "mass_adjustment_fully_upgraded", "percentage_worth", "performance_rating_base",
               "performance_rating_fully_upgraded", "upgrade_department", "upgrade_type"]
TWEAK_FIELDS = ["_metadata_", "cancelling_adaptation.refund_percentage", "cancelling_development.refund_percentage",
                "cancelling_research.refund_percentage", "development_delay.days", "development_delay.thresholds",
                "development_delay.worst_case_cash_penalty", "difficulty_mass_adjustment_scalar",
                "difficulty_mass_adjustment_scalar_enabled",
                "recommended_upgrade.max_percentage_of_upgrades_that_can_be_adapted",
                "recommended_upgrade.min_percentage_of_upgrades_that_can_be_adapted",
                "recommended_upgrade.percentage_chance_of_picking_durability_upgrade",
                "recommended_upgrade.percentage_chance_of_picking_vehicle_upgrade",
                "target_mass_adjustment_at_max_upgrade"]
MODES = ["per_driver_mode", "per_team_mode", "per_driver_unique_mode"]
MODE_FIELDS = ["research_delay_days_min", "research_delay_days_max", "minor_upgrade_cost", "minor_upgrade_cost_to_develop",
               "minor_upgrade_num_days_to_research", "minor_upgrade_num_days_to_develop", "major_upgrade_cost",
               "major_upgrade_cost_to_develop", "major_upgrade_num_days_to_research", "major_upgrade_num_days_to_develop",
               "ultimate_upgrade_cost", "ultimate_upgrade_cost_to_develop", "ultimate_upgrade_num_days_to_research",
               "ultimate_upgrade_num_days_to_develop", "cost_increase_per_upgrade", "research_time_percentage",
               "development_failure_chance_percentage", "research_delay_chance_percentage", "rush_enabled",
               "rush_chance_percentage", "rush_development_time_percentage",
               "rush_development_failure_chance_percentage", "development_time_percentage",
               "max_simultaneous_developments", "upgrade_cost_percentage", "secret_upgrades_viewable_level",
               "practice_programme_points_percentage", "high_inflation", "max_simultaneous_research",
               "development_cost_percentage", "adapting_cost_percentage", "development_speed_penalty_for_repair",
               "dual_development_cost_percentage", "research_cost_percentage", "dual_development_enabled",
               "research_delay_scale", "development_failure_time_percentage"]
WAYPOINT_FIELDS = ["_metadata_", "destination_upgrade_name", "path_length", "path_x_coords", "path_y_coords",
                   "start_upgrade_name"]
SHARED_SET = {"_metadata_": SHARED_1990, "ComAccolades_0.data_group": career.DATA_GROUP,
              "ComResourcePoints_0.data_group": career.DATA_GROUP, "ComSchedule_0.event_list": career.EVENT_LIST,
              "ComUpgrades_0.upgrades_path": PREFIX}

# Resource points by the season pack's reputation: (per-driver start, per-driver weekly, per-team start).
BUDGET = {"TOP_TEAM": (1600, 420, 2000), "MIDFIELD_HIGH": (1300, 350, 1700), "MIDFIELD": (1000, 280, 1300),
          "MINNOW": (750, 220, 1000), "SUPER_MINNOW": (550, 170, 750)}
BUDGET_SPREAD = 0.6
CARS_26 = {"larrousse", "dallara"}

DEPT_STRINGS = {
    "lng_rnd_dept_aero": "Aerodynamics",
    "lng_rnd_dept_chassis": "Chassis & Suspension",
    "lng_rnd_dept_engine": "Engine",
    "lng_rnd_dept_maintenance": "Reliability & Gearbox",
    "lng_rnd_chassis": "Chassis & Suspension",
    "lng_rnd_dept_aero_overview_description":
        "The Aerodynamics department develops the flat-bottom car: nose, wings, diffuser and sidepods. Downforce "
        "costs straight-line speed; tighter sidepods cost cooling.",
    "lng_rnd_dept_chassis_overview_description":
        "The Chassis & Suspension department builds the carbon tub, the suspension and the brakes, takes the car "
        "towards the 500 kg limit and runs the B-spec car and active suspension projects.",
    "lng_rnd_dept_engine_overview_description":
        "The Engine department works with the engine supplier on power, revs and fuel consumption, with no "
        "refuelling allowed. A works engine goes further than a customer one. The qualifying line only works in "
        "qualifying.",
    "lng_rnd_dept_maintenance_overview_description":
        "The Reliability & Gearbox department fights the retirements of 1990: gearbox, engine, cooling and "
        "electrics, and the semi-automatic gearbox project.",
}


def shared_fields(db):
    i = db.row_index(SHARED, "SharedComponents", SHARED_SOURCE)
    fields = sorted(db.probe(f"{SHARED}.SharedComponents_{i}", set()))
    missing = [f for f in SHARED_SET if f not in fields]
    counts = [f for f in fields if f.startswith("_Com") and f.endswith("_instancecount_")]
    if missing or len(counts) != 4:
        sys.exit(f"{SHARED_SOURCE}: fields {missing} missing or {len(counts)} component counts, not 4")
    return fields


def mode_ops(db, token, base):
    ops = []
    src = f"{STOCK}.trees.{token}.department_tweakables_modes_0"
    for mode in MODES:
        for f in MODE_FIELDS:
            v = db.d.get(gamedb.fnv1a64(f"{src}.{mode}.{f}".encode()))
            if v is None:
                continue
            tag, value = v
            size = f.split("_upgrade_")[0].capitalize() if "_upgrade_" in f else None
            if mode == "per_driver_mode" and size in SIZES:
                if f.endswith("_upgrade_cost"):
                    value = SIZES[size]["cost"]
                elif f.endswith("_num_days_to_research"):
                    value = SIZES[size]["research"] + SIZES[size]["develop"]
            ops.append(setop(f"{base}.department_tweakables_modes_0.{mode}", f, value, tag))
    return ops


def djb2(text):
    h = 5381
    for c in text.encode():
        h = (h * 33 + c) & 0xFFFFFFFF
    return h


def setop(select, field, value, tag):
    return {"op": "set", "select": select, "field": field, "value": str(value), "create": True, "type": tag}


def fmt(v):
    return str(int(v)) if float(v).is_integer() else f"{v:.2f}".rstrip("0")


def tree_rows(token):
    works = TOKENS[token][2]
    rows = [u for u in UPGRADES if u.only is None or (u.only == "works" and works)]
    names = {u.name for u in rows}
    return rows, names


def shares(rows):
    total = {}
    for u in rows:
        total[u.type] = total.get(u.type, 0) + float(SIZES[u.size]["weighting"])
    return {u.name: float(SIZES[u.size]["weighting"]) / total[u.type] for u in rows}


def team_mass(team):
    s = Season.load()
    return 575 * s.teams[PACK_TEAM.get(team, team)].weight


def weight_target(team):
    return max(-40.0, min(-10.0, -0.5 * (team_mass(team) - 505)))


TIER = {"TOP_TEAM": 0.10, "MIDFIELD_HIGH": 0.05, "MIDFIELD": 0.0, "MINNOW": -0.03, "SUPER_MINNOW": -0.06}
RATING_RANGE = (0.78, 1.05)


def pace(team):
    t = Season.load().teams[PACK_TEAM.get(team, team)]
    return TIER[t.reputation] + t.power / t.weight / t.drag ** 0.5


def rating_scale(token):
    paces = {k: sum(pace(t) for t in v[0]) / len(v[0]) for k, v in TOKENS.items()}
    lo, hi = min(paces.values()), max(paces.values())
    return RATING_RANGE[0] + (RATING_RANGE[1] - RATING_RANGE[0]) * (paces[token] - lo) / (hi - lo)


def metadata(token, stock):
    teams, supplier, works, _ = TOKENS[token]
    out = {}
    bhp = SUPPLIER_BHP[supplier]
    mass = weight_target(teams[0])
    k = rating_scale(token)
    for name, row in stock.items():
        r = dict(row)
        if name == "engine_power":
            r["bhp_adjustment_fully_upgraded"] = fmt(bhp)
            r["affect_on_vehicle"] = fmt(round(bhp * 1.6))
            r["performance_rating_fully_upgraded"] = fmt(round(112 + 97 * bhp / 74.7, 1))
        elif name == "fuel_consumption":
            r["affect_on_vehicle"] = "-12" if works else "-8"
            r["mass_adjustment_fully_upgraded"] = "-9" if works else "-6"
        elif name == "chassis_weight_reduction":
            r["mass_adjustment_fully_upgraded"] = fmt(round(mass, 1))
            r["affect_on_vehicle"] = fmt(round(mass * 1.4))
        elif name == "front_downforce":
            r["bhp_adjustment_fully_upgraded"] = "-3"
        elif name == "rear_downforce":
            r["bhp_adjustment_fully_upgraded"] = "-6"
        for f in ("performance_rating_base", "performance_rating_fully_upgraded"):
            r[f] = fmt(round(float(r[f]) * k, 1))
        out[name] = r
    return out


def budgets():
    s = Season.load()
    tiers = {}
    for t in s.teams.values():
        tiers.setdefault(t.reputation, []).append(t.reliability)
    out = {}
    for sl in slots():
        t = s.teams[sl.team]
        mean = sum(tiers[t.reputation]) / len(tiers[t.reputation])
        k = 1 + BUDGET_SPREAD * (t.reliability - mean)
        out[career_token(sl.team)] = tuple(int(round(v * k / 10.0)) * 10 for v in BUDGET[t.reputation]) + (t.reputation,)
    return out


def career_token(pack_team):
    return {v: k for k, v in PACK_TEAM.items()}.get(pack_team, pack_team)


def stock_rows(db, path, word, fields):
    n = db.count(path, word)
    rows = {}
    types = {}
    for i in range(n):
        row = {}
        for f in fields:
            v = db.d.get(gamedb.fnv1a64(f"{path}.{word}_{i}.{f}".encode()))
            if v is not None:
                row[f] = v[1]
                types[f] = v[0]
        rows[row["_metadata_"]] = row
    return rows, types


def engine_label(supplier):
    return f"f1990_engine_{supplier}"


def engine_links(db):
    tree = f"{STOCK}.engine_suppliers.trees"
    out = {}
    for i in range(int(db.get(f"{tree}._engine_supplier_upgrades_instancecount_"))):
        name = db.get(f"{tree}.engine_supplier_upgrades_{i}._metadata_")
        labels = [engine_label(s) for s in ENGINE_TREES[name]]
        out[f"{PREFIX}.engine_suppliers.trees.engine_supplier_upgrades_{i}"] = {
            "engines_linked_to": ",".join(labels), "parent_engine": labels[0]}
    linked = sorted(s for v in ENGINE_TREES.values() for s in v)
    if len(out) != len(ENGINE_TREES) or linked != sorted(SUPPLIER_BHP):
        raise ValueError(f"engine supplier trees {sorted(out)} do not cover the suppliers {sorted(SUPPLIER_BHP)}")
    return out


def relink_engines(db, ops):
    links = engine_links(db)
    for op in ops:
        if op.get("field") in ENGINE_LINK_FIELDS and op.get("select") in links:
            op["value"] = links[op["select"]][op["field"]]
    return ops


def copy_subtree(db, sub):
    src = f"{STOCK}.{sub}"
    h = "{%016x}" % gamedb.fnv1a64(src.encode())
    patterns = sorted({k[len(h) + 1:] for k in db.cat if k.startswith(h + ".")} | set(UNCATALOGUED.get(sub, [])))
    keys = set()
    for p in patterns:
        keys.update(db.under(src, p))
    ops = []
    for k in sorted(keys):
        tag, value = db.d[gamedb.fnv1a64(f"{src}.{k}".encode())]
        row, _, field = f"{PREFIX}.{sub}.{k}".rpartition(".")
        ops.append(setop(row, field, value, tag))
    return ops


def build(db):
    meta_stock, meta_types = stock_rows(db, f"{STOCK}.trees.{TEMPLATE_TOKEN}.upgrade_type_metadata",
                                        "upgrade_type_metadata", META_FIELDS)
    row_types = {f: db.d[gamedb.fnv1a64(f"{STOCK}.trees.{TEMPLATE_TOKEN}.aerodynamics.upgrade_0.{f}".encode())][0]
                 for f in ROW_FIELDS}
    tweak_path = f"{STOCK}.trees.{TEMPLATE_TOKEN}.tweakables_0"
    tweaks = {f: db.d[gamedb.fnv1a64(f"{tweak_path}.{f}".encode())] for f in TWEAK_FIELDS}
    wp_types = {f: db.d[gamedb.fnv1a64(
        f"{STOCK}.trees.{TEMPLATE_TOKEN}.connection_waypoint_paths.upgrade_tree_waypoint_path_0.{f}".encode())][0]
        for f in WAYPOINT_FIELDS}
    hub_path = f"{STOCK}.trees.{TEMPLATE_TOKEN}.fake.upgrade_0"
    hub = {f: db.d[gamedb.fnv1a64(f"{hub_path}.{f}".encode())] for f in ROW_FIELDS}
    meta_order = [db.get(f"{STOCK}.trees.{TEMPLATE_TOKEN}.upgrade_type_metadata.upgrade_type_metadata_{i}._metadata_")
                  for i in range(len(meta_stock))]

    ops = []
    ops.append({"op": "add-row", "table": SHARED, "from": f"{SHARED}.SharedComponents[_metadata_={SHARED_SOURCE}]",
                "set": SHARED_SET})
    ops.append({"op": "set", "select": f"championships.Championship[_metadata_={career.CHAMPIONSHIP}]",
                "field": "shared_components", "value": SHARED_LIST})

    for sub in COPIED:
        ops += copy_subtree(db, sub)
    relink_engines(db, ops)

    for token in TOKENS:
        base = f"{PREFIX}.trees.{token}"
        teams = TOKENS[token][0]
        vehicles = [f"{t}_1990{b}" for t in teams for b in ("", "b")]
        ops.append(setop(base, "_teams_linked_to_instancecount_", 1, INT))
        ops.append(setop(f"{base}.teams_linked_to_0", "_metadata_", "teams_linked_to", STR))
        ops.append(setop(f"{base}.teams_linked_to_0", "links", ",".join(vehicles), LIST))
        ops.append(setop(base, "_department_tweakables_modes_instancecount_", 1, INT))
        ops.append(setop(f"{base}.department_tweakables_modes_0", "_metadata_", "_department_tweakables_modes", STR))
        ops += mode_ops(db, token, base)
        ops.append(setop(base, "_tweakables_instancecount_", 1, INT))
        for f, (tag, value) in tweaks.items():
            if f == "target_mass_adjustment_at_max_upgrade":
                value = fmt(round(weight_target(teams[0]) * 115 / 17))
            ops.append(setop(f"{base}.tweakables_0", f, value, tag))

        meta = metadata(token, meta_stock)
        mpath = f"{base}.upgrade_type_metadata"
        ops.append(setop(mpath, "_upgrade_type_metadata_instancecount_", len(meta_order), INT))
        for i, name in enumerate(meta_order):
            for f in META_FIELDS:
                ops.append(setop(f"{mpath}.upgrade_type_metadata_{i}", f, meta[name][f], meta_types[f]))

        rows, names = tree_rows(token)
        for dept, sub in DEPARTMENT_FILES.items():
            dept_rows = [u for u in rows if DEPT[u.type] == dept]
            dpath = f"{base}.{sub}"
            ops.append(setop(dpath, "_upgrade_instancecount_", len(dept_rows), INT))
            for i, u in enumerate(dept_rows):
                s = SIZES[u.size]
                research, develop = u.days or (s["research"], s["develop"])
                values = {
                    "_metadata_": u.label, "upgrade_name": upgrade_name(token, u), "upgrade_type": u.type, "upgrade_size": u.size,
                    "upgrade_department": dept, "cost": u.cost or s["cost"], "cost_to_develop": COST_TO_DEVELOP,
                    "weighting": s["weighting"], "unlock_at_facility_level": 0, "supplier_delivery_date_index": -1,
                    "links": ",".join(f"f1990_{c}" for c in u.links if c in names),
                    "fitting_times.num_days_to_research": research, "fitting_times.num_days_to_develop": develop,
                    "grid_coordinates.x": u.xy[0], "grid_coordinates.y": u.xy[1],
                }
                for f in ROW_FIELDS:
                    ops.append(setop(f"{dpath}.upgrade_{i}", f, values[f], row_types[f]))

        fpath = f"{base}.fake"
        ops.append(setop(fpath, "_upgrade_instancecount_", 1, INT))
        for f in ROW_FIELDS:
            tag, value = hub[f]
            if f == "links":
                value = hub_links()
            ops.append(setop(f"{fpath}.upgrade_0", f, value, tag))

        wpath = f"{base}.connection_waypoint_paths"
        ops.append(setop(wpath, "_upgrade_tree_waypoint_path_instancecount_", len(WAYPOINTS), INT))
        for i, (dest, xs, ys) in enumerate(WAYPOINTS):
            values = {"_metadata_": f"default_to_f1990_{dest}", "destination_upgrade_name": f"f1990_{dest}",
                      "path_length": 4, "path_x_coords": xs, "path_y_coords": ys, "start_upgrade_name": "default"}
            for f in WAYPOINT_FIELDS:
                ops.append(setop(f"{wpath}.upgrade_tree_waypoint_path_{i}", f, values[f], wp_types[f]))

    for team, (start, weekly, team_start, _) in budgets().items():
        sel = f"resource_points.data_group[_metadata_={career.DATA_GROUP}].teams.team[teams_linked_to={team}_1990]"
        when = {"when": {"cars": 26}} if team in CARS_26 else {}
        for field, value in [("per_driver_mode.num_initial_spendable_resource_points", start),
                             ("per_driver_mode.num_spendable_resource_points_awarded_per_week", weekly),
                             ("num_initial_spendable_resource_points", team_start)]:
            ops.append({"op": "set", "select": sel, "field": field, "value": str(value), **when})

    return {"schema_version": 1, "fields": {f"{SHARED}.SharedComponents": shared_fields(db)}, "ops": ops}


def hub_links():
    return ",".join(f"f1990_{r}" for r in ROOTS)


def upgrade_name(token, u):
    team = TOKENS[token][0][0]
    return f"{u.label}_{team}" if (team, u.name) in TEAM_TEXT else u.label


def lang_doc():
    ops = []
    seen = set()
    for u in UPGRADES:
        if u.label in seen:
            continue
        seen.add(u.label)
        ops.append({"op": "lang.set", "language": "*", "key": f"lng_rnd_{u.label}", "value": u.title})
        ops.append({"op": "lang.set", "language": "*", "key": f"lng_rnd_{u.label}_description", "value": u.text})
    for (team, name), (title, text) in TEAM_TEXT.items():
        ops.append({"op": "lang.set", "language": "*", "key": f"lng_rnd_f1990_{name}_{team}", "value": title})
        ops.append({"op": "lang.set", "language": "*", "key": f"lng_rnd_f1990_{name}_{team}_description",
                    "value": text})
    return {"schema_version": 1, "ops": ops}


def strings_doc():
    return {"scope": career.DATA_GROUP, "strings": {"*": DEPT_STRINGS}}


def art_doc():
    frames = {u.label: u.art for u in UPGRADES}
    frames.update({f"f1990_{name}_{team}": next(u.art for u in UPGRADES if u.name == name)
                   for team, name in TEAM_TEXT})
    return {"frames": frames,
            "milestones": {"bspec": "f1990_bspec_package", "bspec_hash": "0x%08x" % djb2("f1990_bspec_package_255")},
            "qualifying": {u.label: "0x%08x" % djb2(f"{u.label}_255") for u in UPGRADES if u.quali}}


def dump_ops(doc):
    head = {k: v for k, v in doc.items() if k != "ops"}
    lines = ["{"] + [f" {json.dumps(k)}: {json.dumps(v, ensure_ascii=False)}," for k, v in head.items()]
    lines.append(' "ops": [')
    lines.append(",\n".join("  " + json.dumps(o, ensure_ascii=False) for o in doc["ops"]))
    lines += [" ]", "}"]
    return "\n".join(lines) + "\n"


def summary():
    lines = []
    for token in TOKENS:
        rows, _ = tree_rows(token)
        sh = shares(rows)
        cost = sum(u.cost or SIZES[u.size]["cost"] for u in rows)
        lines.append(f"{token:13s} {'+'.join(TOKENS[token][0]):18s} {TOKENS[token][1]:12s} rows {len(rows):2d} "
                     f"cost {cost:5d} quali share {sum(sh[u.name] for u in rows if u.quali):.2f}")
    for team, b in budgets().items():
        lines.append(f"budget {team:12s} {b[3]:14s} start {b[0]:5d} weekly {b[1]:4d} team {b[2]:5d}")
    return "\n".join(lines)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 1:
        sys.exit(__doc__)
    db = career.Db(args[0])
    outputs = [(OUT_DB, dump_ops(build(db))), (OUT_LANG, dump_ops(lang_doc())),
               (OUT_STRINGS, json.dumps(strings_doc(), indent=1, ensure_ascii=False) + "\n"),
               (OUT_ART, json.dumps(art_doc(), indent=1) + "\n")]
    differ = []
    for path, text in outputs:
        old = open(path, encoding="utf-8").read() if os.path.exists(path) else None
        if old != text:
            differ.append(path)
            if "--check" not in sys.argv:
                with open(path, "w", encoding="utf-8") as f:
                    f.write(text)
    print(summary())
    for p in differ:
        print(("differs: " if "--check" in sys.argv else "wrote ") + os.path.relpath(p, ADDON))
    return 1 if differ and "--check" in sys.argv else 0


if __name__ == "__main__":
    sys.exit(main())
