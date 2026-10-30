#!/usr/bin/env python3
"""Writes db/grid.json and lang/grid.json: the 1990 teams, drivers and entrant lists, all as new rows.

usage: grid.py <stock gamedatabases.2025.bin> [-v]
"""
import json
import re
import sys
from pathlib import Path

import career
import tracks
from season import Season, slots

ROOT = Path(__file__).resolve().parents[1]
NAMES = ROOT.parents[1] / "tools/gamedb/names.txt"
DM_FIELDS = ROOT.parent / "driver-manager/launcher/src/fields.rs"

TEAMS = "entrants.teams"
DRIVERS = "entrants.drivers"
CHARACTERS = "entrants.characters"
LOOK_FOLDER = "entrants.driver_customisations.folder_1"
LOOKS = "entrants.driver_customisations.folder[_metadata_=official_driver_customisations]"
LOOK_WORD = "preloaded_driver_customisation_properties"
STATS_TABLE = "driver_statistics.driver_statistics"
ENTRANTS = "entrants.entrant_lists.entrants"
LISTS = "entrants.entrant_lists"
SETUP_MODES = "entrants.entrant_setup_modes"
STANDINGS = "entrants.standings"
KIT_YEARS = "entrants.team_customisation"
TRACK_LAYERS = "track_data.track_layers"
TRACK_LAYER_STAT_ID = 14
KITS = f"{KIT_YEARS}.team_customisation_for_year[_metadata_=2026]"

SETUP_MODE = "f1_1990"
GRID_LIST = "f1_1990"
CONTENT_SETS = "content_sets"
CONTENT_SET = "2026"
POOL_LIST = "f1_1990_driver_pool"
CAREER_LIST = career.ENTRANT_LIST
DATA_GROUP = career.DATA_GROUP
PARTS = ("grid", "grid_standings", "grid_links")
GAME_SETUP_MODES = "game_setup.game_setup_modes.game_setup_mode"
TEAM_STANDINGS = "f1_teams_1989"
DRIVER_STANDINGS = "f1_drivers_1989"

TEAM_TEMPLATE = "ferrari_26"
DRIVER_TEMPLATE = "lando_norris"
LOOK_TEMPLATE = "2026-mclaren_lando_norris"
LIST_TEMPLATE = "f1_2026"
ENTRANT_TEMPLATE = "jarno_opmeer"
HOLDER_TEMPLATE = "nyck_de-vries"
MODE_TEMPLATE = "f1_26"
KIT_TEMPLATE = "2026-default_team_customisation"
TEAM_STANDINGS_TEMPLATE = "f1_teams_2026"
DRIVER_STANDINGS_TEMPLATE = "f1_drivers_2025"

TOKEN = {"ligier_prost": "ligier", "march": "leytonhouse"}
DONOR_ROW = {"team12_26": "williams_26", "team13_26": "haas_26"}
EXTRA_TEAMS = ("larrousse", "dallara")
TEAM_STAT_IDS = range(502, 515)
DRIVER_IDS_KEPT_FREE = range(208, 235)

SPARE_LOOKS = [
    "2026-f2-aixracing_cian_shields", "2026-f2-artgrandprix_kush_maini", "2026-f2-camposracing_nikola_tsolov",
    "2026-f2-dams_dino_beganovic", "2026-f2-hitech_ritomo_miyata", "2026-f2-invictaracing_joshua_duerksen",
    "2026-f2-mpmotorsport_gabriele_mini", "2026-f2-mpmotorsport_oliver_goethe", "2026-f2-prema_sebastian_montoya",
    "2026-f2-tridentmotorsport_john_bennett",
]
PACKAGES = {
    "mclaren_26": ["lando_norris", "oscar_piastri"],
    "mercedes_26": ["george_russell", "andrea-kimi_antonelli"],
    "redbull_26": ["max_verstappen", "isack_hadjar"],
    "ferrari_26": ["charles_leclerc", "lewis_hamilton"],
    "williams_26": ["alex_albon", "carlos_sainz-jr"],
    "tororosso_26": ["liam_lawson", "arvid_lindblad"],
    "forceindia_26": ["fernando_alonso", "lance_stroll"],
    "haas_26": ["oliver_bearman", "esteban_ocon"],
    "sauber_26": ["nico_hulkenberg", "gabriel_bortoleto"],
    "renault_26": ["pierre_gasly", "franco_colapinto"],
    "gm_cadillac_26": ["sergio_perez", "valtteri_bottas"],
}
LOOK_TEAM = {"gm_cadillac_26": "cadillac"}
PACKAGE_LOOK = {d: f"2026-{LOOK_TEAM.get(row, row[:-3])}_{d}" for row, ds in PACKAGES.items() for d in ds}
OTHER_ROWS = ["nyck_de-vries", "marino_sato", "marcus_armstrong", "juri_vips", "felipe_drugovich", "olli_caldwell"]
STOCK = {
    "ayrton_senna": ("ayrton_senna", "classic-senna01_ayrton_senna"),
    "alain_prost": ("alain_prost", "tier01_livery85"),
    "nigel_mansell": ("nigel_mansell", "tier04_livery86"),
}
MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
          "November", "December"]
STATS = ("experience", "racecraft", "awareness", "pace", "focus")
TRAITS = ("aggression", "control", "mistakes", "reaction_speed", "speed_in", "speed_out")

DRIVER_INFO = {
    "ayrton_senna": ("SEN", 1984, (1960, 3, 21), None, None),
    "alain_prost": ("PRO", 1980, (1955, 2, 24), "male_ls347", None),
    "nigel_mansell": ("MAN", 1980, (1953, 8, 8), "male_ls144", None),
    "gerhard_berger": ("BER", 1984, (1959, 8, 27), "male_ls204", "tier03_livery101"),
    "thierry_boutsen": ("BOU", 1983, (1957, 7, 13), "male_ls222", "tier01_livery76"),
    "riccardo_patrese": ("PAT", 1977, (1954, 4, 17), "male_ls240", "tier02_livery95"),
    "alessandro_nannini": ("NAN", 1986, (1959, 7, 7), "male_ls248", "tier02_livery104"),
    "nelson_piquet": ("PIQ", 1978, (1952, 8, 17), "male_ls192", "tier01_livery79"),
    "satoru_nakajima": ("NAK", 1987, (1953, 2, 23), "male_ls345", "tier05_livery73"),
    "jean_alesi": ("ALE", 1989, (1964, 6, 11), "male_ls247", "tier04_livery103"),
    "derek_warwick": ("WAR", 1981, (1954, 8, 27), "male_ls356", "tier01_livery75"),
    "martin_donnelly": ("DON", 1989, (1964, 3, 26), "male_ls526", "tier02_livery82"),
    "mauricio_gugelmin": ("GUG", 1988, (1963, 4, 20), "male_ls758", "tier03_livery105"),
    "ivan_capelli": ("CAP", 1985, (1963, 5, 24), "male_ls161", "tier01_livery80"),
    "eric_bernard": ("BRN", 1989, (1964, 8, 24), "male_ls212", "tier01_livery78"),
    "aguri_suzuki": ("SUZ", 1988, (1960, 9, 8), "male_ls197", "tier02_livery103"),
    "nicola_larini": ("LAR", 1987, (1964, 3, 19), "male_ls235", "tier03_livery99"),
    "philippe_alliot": ("ALL", 1984, (1954, 7, 27), "male_ls343", "tier02_livery01"),
    "pierluigi_martini": ("MAR", 1984, (1961, 4, 23), "male_ls807", "tier01_livery52"),
    "paolo_barilla": ("BAR", 1989, (1961, 4, 20), "male_ls206", "tier01_livery88"),
    "gregor_foitek": ("FOI", 1989, (1965, 3, 27), "male_ls247", "tier01_livery77"),
    "stefano_modena": ("MOD", 1987, (1963, 5, 12), "male_ls265", "tier03_livery107"),
    "gianni_morbidelli": ("MOR", 1989, (1968, 1, 13), "male_ls553", "tier01_livery86"),
    "andrea_de_cesaris": ("DEC", 1980, (1959, 5, 31), "male_ls683", "tier02_livery94"),
    "michele_alboreto": ("ALB", 1981, (1956, 12, 23), "male_ls766", "tier04_livery105"),
    "bernd_schneider": ("SCH", 1988, (1964, 7, 20), "male_ls790", "tier04_livery20"),
    "david_brabham": ("BRA", 1990, (1965, 9, 5), "male_ls752", "tier04_livery94"),
    "emanuele_pirro": ("PIR", 1989, (1962, 1, 12), "male_ls533", "tier03_livery106"),
    "alex_caffi": ("CAF", 1986, (1964, 3, 18), "male_ls515", "tier05_livery74"),
    "roberto_moreno": ("MRN", 1982, (1959, 2, 11), "male_ls487", "tier02_livery84"),
    "johnny_herbert": ("HER", 1989, (1964, 6, 25), "male_ls355", "tier04_livery99"),
}
VOICES = {"johnny_herbert": "Name175"}

TEAM_INFO = {
    "mclaren": ("McLaren Honda", "McLaren", "MCL", ("#E2001A", "#FFFFFF", "#1A1A1A"), "Woking, UK", "britain",
                "MP4/5B", "Honda V10", "Ron Dennis"),
    "ferrari": ("Ferrari", "Ferrari", "FER", ("#A71F22", "#FFFFFF", "#1A1A1A"), "Maranello, Italy", "italy",
                "641", "Ferrari V12", "Cesare Fiorio"),
    "williams": ("Williams Renault", "Williams", "WIL", ("#16427C", "#FFFFFF", "#F2C200"), "Didcot, UK", "britain",
                 "FW13B", "Renault V10", "Frank Williams"),
    "benetton": ("Benetton Ford", "Benetton", "BEN", ("#1F9A48", "#FFFFFF", "#1E4FA0"), "Witney, UK", "britain",
                 "B190", "Ford V8", "Flavio Briatore"),
    "tyrrell": ("Tyrrell Ford", "Tyrrell", "TYR", ("#1F2F6B", "#FFFFFF", "#8AB4E8"), "Ockham, UK", "britain",
                "019", "Ford V8", "Ken Tyrrell"),
    "lotus": ("Lotus Lamborghini", "Lotus", "LOT", ("#F2C500", "#1F4E9C", "#FFFFFF"), "Hethel, UK", "britain",
              "102", "Lamborghini V12", "Tony Rudd"),
    "march": ("Leyton House Judd", "Leyton House", "LEY", ("#5CDEE5", "#1B2A5C", "#FFFFFF"), "Bicester, UK",
              "britain", "CG901", "Judd V8", "Ian Phillips"),
    "larrousse": ("Larrousse Lamborghini", "Larrousse", "LAR", ("#5FAA68", "#FFFFFF", "#1F3C88"), "Antony, France",
                  "france", "LC90", "Lamborghini V12", "Gérard Larrousse"),
    "ligier_prost": ("Ligier Ford", "Ligier", "LIG", ("#0F5FDD", "#FFFFFF", "#6EB4FF"), "Magny-Cours, France",
                     "france", "JS33B", "Ford V8", "Guy Ligier"),
    "minardi": ("Minardi Ford", "Minardi", "MIN", ("#F7AC15", "#FFFFFF", "#1A1A1A"), "Faenza, Italy", "italy",
                "M190", "Ford V8", "Giancarlo Minardi"),
    "brabham": ("Brabham Judd", "Brabham", "BRA", ("#2A3052", "#FFFFFF", "#C8102E"), "Chessington, UK", "britain",
                "BT59", "Judd V8", "Herbie Blash"),
    "dallara": ("Dallara Ford", "Dallara", "DAL", ("#960D04", "#FFFFFF", "#1A1A1A"), "Lumezzane, Italy", "italy",
                "F190", "Ford V8", "Giuseppe Lucchini"),
    "arrows": ("Arrows Ford", "Arrows", "ARR", ("#FFFFFF", "#C8102E", "#1A3C8C"), "Milton Keynes, UK", "britain",
               "A11B", "Ford V8", "Jackie Oliver"),
}
UI_COLOUR = {"arrows": "#C8102E"}
STORY = {
    "mclaren": ("Senna and Berger drive the Honda V10 MP4/5B for the reigning champions.", "#McLaren"),
    "ferrari": ("Prost joins Mansell at Maranello, with the 641 and its semi-automatic gearbox.", "#Ferrari"),
    "williams": ("Renault V10 power and the FW13B for Boutsen and Patrese.", "#Williams"),
    "benetton": ("John Barnard's B190 with Ford power for Nannini and Piquet.", "#Benetton"),
    "tyrrell": ("Alesi and Nakajima drive; Harvey Postlethwaite's raised-nose 019 arrives at Imola.", "#Tyrrell"),
    "lotus": ("Lamborghini V12 power for Warwick and Donnelly in the 102.", "#Lotus"),
    "march": ("Adrian Newey's CG901 with Judd power for Gugelmin and Capelli.", "#LeytonHouse"),
    "larrousse": ("The Lola-built LC90 with the Lamborghini V12 for Bernard and Suzuki.", "#Larrousse"),
    "ligier_prost": ("The JS33B with Ford power for Larini and Alliot.", "#Ligier"),
    "minardi": ("The Faenza team runs the M190 with Ford power for Martini and Barilla.", "#Minardi"),
    "brabham": ("The BT59 with Judd power for Modena and David Brabham.", "#Brabham"),
    "dallara": ("BMS Scuderia Italia runs the Dallara F190 for de Cesaris and Pirro.", "#ScuderiaItalia"),
    "arrows": ("Arrows runs the A11B with Ford power for Alboreto and Caffi.", "#Arrows"),
}
SUIT = "tier01_livery76"
BOSS_ROWS = {"mclaren_26": "andrea_stella", "mercedes_26": "toto_wollf", "redbull_26": "laurent_mekies",
             "ferrari_26": "frederic_vasseur", "williams_26": "james_vowles", "tororosso_26": "alan_permane",
             "forceindia_26": "adrian_newey", "haas_26": "ayao_komatsu", "sauber_26": "mattia_binotto",
             "renault_26": "steve_nielsen", "gm_cadillac_26": "graeme_lowdon"}
KIT_ROWS = {"mclaren_26": "2026-mclaren", "ferrari_26": "2026-ferrari", "williams_26": "2026-williams",
            "renault_26": "2026-renault", "mercedes_26": "2026-mercedes", "redbull_26": "2026-redbull",
            "forceindia_26": "2026-forceindia", "sauber_26": "2026-sauber", "tororosso_26": "2026-tororosso",
            "gm_cadillac_26": "2026-cadillac", "haas_26": "2026-haas"}
GROUP_TEAM_TABLES = [
    ("accolades", "AccoladeDataGroup", "team_data", "AccoladeTeam", "team"),
    ("career.my_team_sponsors", "MyTeamSponsorsDataGroup", "team_primary_sponsors", "team_primary_sponsor", "team"),
    ("recognition", "RecognitionDataGroup", "team_modifiers", "TeamModifier", "team"),
    ("resource_points", "data_group", "teams", "team", "teams_linked_to"),
]
STANDINGS_1989 = ["mclaren", "williams", "ferrari", "benetton", "tyrrell", "lotus", "arrows", "brabham", "minardi",
                  "march", "ligier_prost", "larrousse", "dallara"]
DRIVERS_1989 = ["alain_prost", "ayrton_senna", "riccardo_patrese", "nigel_mansell", "thierry_boutsen",
                "alessandro_nannini", "gerhard_berger", "nelson_piquet", "jean_alesi", "derek_warwick",
                "michele_alboreto", "stefano_modena", "pierluigi_martini", "andrea_de_cesaris", "mauricio_gugelmin",
                "satoru_nakajima", "philippe_alliot", "ivan_capelli", "nicola_larini", "eric_bernard",
                "aguri_suzuki", "martin_donnelly"]


def token(team):
    return TOKEN.get(team, team)


def team_row(team):
    return f"{token(team)}_1990"


def driver_row(did):
    return f"f1990_{did}"


def argb(hex_colour):
    v = int(hex_colour.lstrip("#"), 16) | 0xFF000000
    return str(v - (1 << 32))


def num(v, places=3):
    t = f"{max(0.0, min(1.0, v)):.{places}f}".rstrip("0").rstrip(".")
    return t or "0"


def curve(v):
    v = num(v)
    return f"@v2@minx:0,maxx:10,miny:0,maxy:100,0:{v},0.25:{v},0.5:{v},1:{v}"


def mean(*xs):
    return sum(xs) / len(xs)


def ratings(season, did):
    n = lambda k: season.normalised(did, k)
    seasons = 1990 - DRIVER_INFO[did][1]
    stats = {
        "experience": 0.62 + 0.33 * min(seasons / 10, 1.0),
        "racecraft": 0.70 + 0.27 * mean(n("race_skill"), n("defending"), n("aggression")),
        "awareness": 0.70 + 0.27 * mean(n("avoidance_of_mistakes"), n("avoidance_of_forced_mistakes"),
                                        n("blue_flag_conceding")),
        "pace": 0.70 + 0.27 * mean(n("qualifying_skill"), n("race_skill")),
        "focus": 0.70 + 0.27 * mean(n("stamina"), n("tyre_management"), n("fuel_management"),
                                    n("weather_tyre_changes")),
    }
    traits = {
        "aggression": 0.55 + 0.45 * n("aggression"),
        "control": 0.55 + 0.45 * mean(n("race_skill"), n("wet_skill")),
        "mistakes": 0.55 + 0.45 * mean(n("avoidance_of_mistakes"), n("avoidance_of_forced_mistakes")),
        "reaction_speed": 0.55 + 0.45 * n("start_reactions"),
        "speed_in": 0.55 + 0.45 * n("qualifying_skill"),
        "speed_out": 0.55 + 0.45 * mean(n("qualifying_skill"), n("race_skill")),
    }
    return stats, traits


class Stock:
    def __init__(self, path):
        self.db = career.Db(path)
        self.cache = {}
        names = set()
        for key in self.db.cat:
            parts = key.split(".")
            names.update(".".join(parts[i + 1:]) for i, p in enumerate(parts) if p.endswith("_N"))
        names.update(line.strip() for line in open(NAMES, encoding="utf-8"))
        names.update(re.findall(r'"([a-z_0-9.]+)"', DM_FIELDS.read_text(encoding="utf-8")))
        self.names = sorted(n for n in names if n and "_N" not in n)

    def get(self, path):
        return self.db.get(path)

    def count(self, container, word):
        return self.db.count(container, word) or 0

    def row(self, container, word, label, field="_metadata_"):
        for i in range(self.count(container, word)):
            if self.get(f"{container}.{word}_{i}.{field}") == label:
                return f"{container}.{word}_{i}"
        raise SystemExit(f"{container}.{word}[{field}={label}] not in the stock database")

    def fields(self, row):
        if row not in self.cache:
            template = re.sub(r"_\d+(?=\.|$)", "_N", row) + "."
            out = set()
            for key in self.db.cat:
                if key.startswith(template):
                    out.update(self.db.under(row, key[len(template):]))
            out.update(n for n in self.names if self.get(f"{row}.{n}") is not None)
            self.cache[row] = sorted(out)
        return self.cache[row]

    def values(self, row):
        out = {f: self.get(f"{row}.{f}") for f in self.fields(row)}
        return {f: v for f, v in out.items() if v is not None}

    def free_ids(self, container, word, start, stop):
        used = {int(self.get(f"{container}.{word}_{i}.stat_id")) for i in range(self.count(container, word))
                if self.get(f"{container}.{word}_{i}.stat_id") not in (None, "")}
        return [i for i in range(start, stop) if i not in used]


class Ops:
    def __init__(self, stock, parts):
        self.stock = stock
        self.parts = {name: {"schema_version": 1, "fields": {}, "ops": []} for name in parts}
        self.use(parts[0])

    def use(self, part):
        self.ops, self.fields = self.parts[part]["ops"], self.parts[part]["fields"]

    def set(self, select, field, value, when=None):
        op = {"op": "set", "select": select, "field": field, "value": str(value)}
        if when:
            op["when"] = when
        self.ops.append(op)

    def add(self, table, word, source_row, source, values, when=None):
        fields = self.stock.fields(source_row)
        missing = [k for k in values if k not in fields]
        if missing:
            raise SystemExit(f"{table}.{word}: fields not on {source}: {missing}")
        key = f"{table}.{word}"
        if self.fields.setdefault(key, fields) != fields:
            raise SystemExit(f"{key}: two sources with different fields")
        op = {"op": "add-row", "table": table, "from": source, "set": {k: str(v) for k, v in values.items()}}
        if when:
            op["when"] = when
        self.ops.append(op)

    def clone(self, table, word, template, donor, values, when=None, container=None):
        container = container or table
        t_row, d_row = self.stock.row(container, word, template), self.stock.row(container, word, donor)
        donor_values = self.stock.values(d_row)
        extra = [k for k in donor_values if k not in self.stock.fields(t_row)]
        if extra:
            raise SystemExit(f"{table}.{word}: {donor} has fields {template} lacks: {extra}")
        self.add(table, word, t_row, f"{table}.{word}[_metadata_={template}]", {**donor_values, **values}, when)


def lineup_of(season):
    teams = [s.team for s in slots()]
    regulars = season.regulars(teams)
    stand_ins = season.stand_ins(teams)
    lineup = [(s, pos, seat) for s in slots() for pos, seat in enumerate(regulars[s.team])]
    lineup = [x for x in lineup if x[0].team not in EXTRA_TEAMS] + [x for x in lineup if x[0].team in EXTRA_TEAMS]
    team_of = {seat.driver: s for s, _, seat in lineup}
    number_of = {seat.driver: seat.number for _, _, seat in lineup}
    for d, tid in stand_ins:
        team_of[d] = next(s for s in slots() if s.team == tid)
        for r in season.races:
            hit = next((x.number for x in season.lineup(r.round, [tid])[tid] if x.driver == d), None)
            if hit is not None:
                number_of.setdefault(d, hit)
                break
    return lineup, [d for d, _ in stand_ins], team_of, number_of, regulars


def donors(lineup, free):
    packages = {r: list(ds) for r, ds in PACKAGES.items()}
    others = iter(OTHER_ROWS)
    out = {}
    for s, _, seat in lineup:
        if seat.driver in STOCK:
            out[seat.driver] = STOCK[seat.driver][0]
        else:
            avail = packages.get(s.team_row)
            out[seat.driver] = avail.pop(0) if avail else next(others)
    leftover = [d for r in PACKAGES for d in packages[r]]
    for did in free:
        out[did] = leftover.pop(0) if leftover else next(others)
    return out


def free_list_ids(stock, n):
    """n entrant list stat ids above the stock ones. The game indexes entrant lists by stat id (a career
    setup's data group is the stat id of the entrant list its entrant_list names, 0x1424286e0, kept in a byte),
    so a clone that keeps its template's id resolves to the template."""
    top = max(int(stock.get(f"{LISTS}.entrant_list_{i}.stat_id")) for i in range(stock.count(LISTS, "entrant_list")))
    ids = stock.free_ids(LISTS, "entrant_list", top + 1, 256)[:n]
    if len(ids) < n:
        sys.exit("no free entrant list stat ids below 256")
    return ids


def free_setup_mode_id(stock):
    """A stat id above the stock entrant setup modes: the game keeps them indexed by stat id
    (Sess_Register_EntrantSetupMode), and the f1_26 template's id would collide."""
    top = max(int(stock.get(f"{SETUP_MODES}.entrant_setup_mode_{i}.stat_id"))
              for i in range(stock.count(SETUP_MODES, "entrant_setup_mode")))
    return stock.free_ids(SETUP_MODES, "entrant_setup_mode", top + 1, 256)[0]


def build(stock, season):
    o = Ops(stock, PARTS)
    lang = []
    lineup, free, team_of, number_of, regulars = lineup_of(season)
    seated = [seat.driver for _, _, seat in lineup]
    gated = lambda team: {"cars": 26} if team in EXTRA_TEAMS else None

    def L(key, value):
        lang.append({"op": "lang.set", "language": "*", "key": key, "value": value})

    team_ids = iter(TEAM_STAT_IDS)
    boss_ids = iter(stock.free_ids(CHARACTERS, "character", career.AGENT_1990_STAT_ID + 1, 256))
    for s in slots():
        donor = DONOR_ROW.get(s.team_row, s.team_row)
        name, short, code, colours, base, nat, chassis, engine, boss = TEAM_INFO[s.team]
        key = f"lng_f1990_team_{s.team}"
        L(key, name)
        L(key + "_short", short)
        L(f"lng_f1990_base_{s.team}", base)
        L(f"lng_f1990_chassis_{s.team}", chassis)
        L(f"lng_f1990_engine_{s.team}", engine)
        L(f"lng_f1990_desc_{s.team}", STORY[s.team][0])
        L(f"lng_f1990_tag_{s.team}", STORY[s.team][1])
        boss_row = f"f1990_boss_{token(s.team)}"
        first, _, last = boss.partition(" ")
        for part, value in (("full", boss), ("first", first), ("last", last)):
            L(f"lng_f1990_boss_{s.team}_{part}", value)
        o.clone(CHARACTERS, "character", BOSS_ROWS["mclaren_26"], BOSS_ROWS[donor], {
            "_metadata_": boss_row, "stat_id": next(boss_ids), "full_name": f"lng_f1990_boss_{s.team}_full",
            "first_name": f"lng_f1990_boss_{s.team}_first", "last_name": f"lng_f1990_boss_{s.team}_last",
        })
        p, sec, ter = (argb(c) for c in colours)
        o.clone(TEAMS, "team", TEAM_TEMPLATE, donor, {
            "_metadata_": team_row(s.team), "stat_id": next(team_ids),
            "display_name": key, "display_name_short": key + "_short", "display_name_super_short": key + "_short",
            "display_name_code": code, "primary_colour": p, "secondary_colour": sec, "tertiary_colour": ter,
            "ui_team_colour": argb(UI_COLOUR.get(s.team, colours[0])), "team_wear_primary_colour": p,
            "team_wear_secondary_colour": sec, "team_wear_tertiary_colour": ter,
            "team_base": f"lng_f1990_base_{s.team}", "nationality": nat, "team_boss": boss_row,
            "livery_name": team_row(s.team), "timing_tower_texture_path": team_row(s.team),
            "_story_info_instancecount_": 1, "story_info_0._metadata_": "story_info_0",
            "story_info_0.description": f"lng_f1990_desc_{s.team}",
            "story_info_0.hashtag": f"lng_f1990_tag_{s.team}",
        })

    donor_of = donors(lineup, free)
    driver_ids = stock.free_ids(DRIVERS, "driver", 1, DRIVER_IDS_KEPT_FREE.start)
    driver_ids += stock.free_ids(DRIVERS, "driver", DRIVER_IDS_KEPT_FREE.stop, 255)[::-1]
    look_ids = iter(stock.free_ids(LOOK_FOLDER, LOOK_WORD, 366, 1000))
    spare_looks = iter(SPARE_LOOKS)
    holder_row = stock.row(STATS_TABLE, "DriverStatisticsHolder", HOLDER_TEMPLATE)
    look_template = stock.row(LOOK_FOLDER, LOOK_WORD, LOOK_TEMPLATE)
    entrant_template = stock.row(ENTRANTS, "entrant", ENTRANT_TEMPLATE)
    looks_of_team, report = {}, {"rows": {}, "ratings": {}}
    for i, did in enumerate(seated + free):
        d = season.drivers[did]
        code, debut, (by, bm, bd), head, helmet = DRIVER_INFO[did]
        label = driver_row(did)
        s = team_of[did]
        stats, traits = ratings(season, did)
        report["rows"][did] = (label, donor_of[did])
        report["ratings"][did] = (stats, traits)
        L(f"lng_f1990_{did}_first", d.first)
        L(f"lng_f1990_{did}_last", d.last.upper())
        L(f"lng_f1990_{did}_full", f"{d.first} {d.last.upper()}")
        values = {
            "_metadata_": label, "stat_id": driver_ids[i], "audio_name": VOICES.get(did, "DefaultName"),
            "available": "True", "car_number": number_of[did],
            "date_of_birth.year": by, "date_of_birth.month": MONTHS[bm - 1], "date_of_birth.day": bd,
            "f1_seasons_completed": 1990 - debut, "first_name": f"lng_f1990_{did}_first",
            "full_name": f"lng_f1990_{did}_full", "initials": code, "is_name_reversed": "False",
            "last_name": f"lng_f1990_{did}_last", "nationality": d.nationality,
        }
        if head:
            values["head"] = head
        for t in TRAITS:
            values[f"driver_characteristics.{t}"] = num(traits[t], 2)
        o.clone(DRIVERS, "driver", DRIVER_TEMPLATE, donor_of[did], values)
        for year in career.CAREER_YEARS:
            o.set(f"{DRIVERS}.driver[_metadata_={label}].date_of_birth", "year", by + year - 1990,
                  when={"career_year": str(year)})

        holder = {"_metadata_": label, "driver": label}
        for k, stat in enumerate(STATS):
            holder[f"StatisticsDataSet_0.Statistic_{k}.evolution"] = curve(stats[stat])
        o.add(STATS_TABLE, "DriverStatisticsHolder", holder_row,
              f"{STATS_TABLE}.DriverStatisticsHolder[_metadata_={HOLDER_TEMPLATE}]", holder)

        look = f"f1990-{token(s.team)}_{did}"
        looks_of_team.setdefault(s.team, []).append(look)
        if did in STOCK:
            helmet = STOCK[did][1]
        donor_look = stock.row(LOOK_FOLDER, LOOK_WORD, PACKAGE_LOOK.get(donor_of[did]) or next(spare_looks))
        o.add(LOOKS, LOOK_WORD, look_template, f"{LOOKS}.{LOOK_WORD}[_metadata_={LOOK_TEMPLATE}]", {
            **stock.values(donor_look), "_metadata_": look, "stat_id": next(look_ids), "helmet": helmet,
            "suit": SUIT, "gloves": SUIT, "linked_driver_stat_id": label,
        })

        o.add(ENTRANTS, "entrant", entrant_template, f"{ENTRANTS}.entrant[_metadata_={ENTRANT_TEMPLATE}]", {
            "_metadata_": f"f1990-{did}", "badge_customisation": "", "car_number": number_of[did], "driver": label,
            "driver_customisation": look, "entrant_reference": "", "team": "", "team_position": -1,
            "vehicle": "", "vehicle_customisation": "",
        })

    list_row = stock.row(LISTS, "entrant_list", LIST_TEMPLATE)
    list_from = f"{LISTS}.entrant_list[_metadata_={LIST_TEMPLATE}]"
    grid_id, pool_id, career_id = free_list_ids(stock, 3)
    grid = f"{LISTS}.entrant_list[_metadata_={GRID_LIST}]"
    pool = f"{LISTS}.entrant_list[_metadata_={POOL_LIST}]"
    template_seats = stock.count(list_row, "entrant")
    o.add(LISTS, "entrant_list", list_row, list_from, {
        "_metadata_": GRID_LIST, "stat_id": grid_id, "previous_season_team_standings": TEAM_STANDINGS,
        "previous_season_driver_standings": DRIVER_STANDINGS,
    })
    for n, (s, pos, seat) in enumerate(lineup):
        values = {
            "_metadata_": f"f1990_{token(s.team)}_{pos + 1}", "team": team_row(s.team), "vehicle": team_row(s.team),
            "entrant_reference": f"f1990-{seat.driver}", "car_number": seat.number, "team_position": pos,
            "driver": "", "driver_customisation": "", "badge_customisation": "", "vehicle_customisation": "",
        }
        if n < template_seats:
            for f, v in values.items():
                o.set(f"{grid}.entrant_{n}", f, v)
        else:
            o.add(grid, "entrant", f"{list_row}.entrant_0", f"{grid}.entrant_0", values, gated(s.team))
    o.set(grid, "_entrant_instancecount_", sum(1 for s, _, _ in lineup if s.team not in EXTRA_TEAMS))
    o.set(grid, "_entrant_instancecount_", len(lineup), {"cars": 26})

    o.add(LISTS, "entrant_list", list_row, list_from, {"_metadata_": POOL_LIST, "stat_id": pool_id})
    for n, did in enumerate(free):
        for f, v in {"_metadata_": f"f1990_pool_{did}", "entrant_reference": f"f1990-{did}", "team": "",
                     "vehicle": "", "team_position": -1, "car_number": 255, "driver": "",
                     "driver_customisation": ""}.items():
            o.set(f"{pool}.entrant_{n}", f, v)
    o.set(pool, "_entrant_instancecount_", len(free))

    o.add(LISTS, "entrant_list", list_row, list_from, {
        "_metadata_": CAREER_LIST, "stat_id": career_id, "entrant_list_references": f"{GRID_LIST},{POOL_LIST}",
        "previous_season_team_standings": TEAM_STANDINGS, "previous_season_driver_standings": DRIVER_STANDINGS,
        "_entrant_instancecount_": 0,
    })

    layer_row = stock.row(TRACK_LAYERS, "TrackLayer", MODE_TEMPLATE)
    o.add(TRACK_LAYERS, "TrackLayer", layer_row, f"{TRACK_LAYERS}.TrackLayer[_metadata_={MODE_TEMPLATE}]", {
        "_metadata_": SETUP_MODE, "game_setup_modes": SETUP_MODE, "statId": TRACK_LAYER_STAT_ID,
        "worldAssetGroup": stock.get(f"{stock.row(TRACK_LAYERS, 'TrackLayer', 'myteam_26')}.worldAssetGroup"),
        "worldEri": tracks.TRACK_LAYER,
    })

    mode_row = stock.row(SETUP_MODES, "entrant_setup_mode", MODE_TEMPLATE)
    o.add(SETUP_MODES, "entrant_setup_mode", mode_row, f"{SETUP_MODES}.entrant_setup_mode[_metadata_={MODE_TEMPLATE}]", {
        "_metadata_": SETUP_MODE, "stat_id": free_setup_mode_id(stock), "entrant_lists": GRID_LIST,
        "selectable_entrant_lists": GRID_LIST, "max_entrants": "{{cars}}",
    })

    team_std = stock.row(STANDINGS, "standings", TEAM_STANDINGS_TEMPLATE)
    o.add(STANDINGS, "standings", team_std, f"{STANDINGS}.standings[_metadata_={TEAM_STANDINGS_TEMPLATE}]",
          {"_metadata_": TEAM_STANDINGS})
    tstd = f"{STANDINGS}.standings[_metadata_={TEAM_STANDINGS}]"
    refs = stock.count(team_std, "team_reference")
    for n, team in enumerate(STANDINGS_1989):
        values = {"_metadata_": token(team), "team": team_row(team)}
        if n < refs:
            for f, v in values.items():
                o.set(f"{tstd}.team_reference_{n}", f, v)
        else:
            o.add(tstd, "team_reference", f"{team_std}.team_reference_0", f"{tstd}.team_reference_0", values,
                  gated(team))
    o.set(tstd, "_team_reference_instancecount_", sum(1 for t in STANDINGS_1989 if t not in EXTRA_TEAMS))
    o.set(tstd, "_team_reference_instancecount_", len(STANDINGS_1989), {"cars": 26})

    o.use("grid_standings")
    drv_std = stock.row(STANDINGS, "standings", DRIVER_STANDINGS_TEMPLATE)
    o.add(STANDINGS, "standings", drv_std, f"{STANDINGS}.standings[_metadata_={DRIVER_STANDINGS_TEMPLATE}]",
          {"_metadata_": DRIVER_STANDINGS})
    dstd = f"{STANDINGS}.standings[_metadata_={DRIVER_STANDINGS}]"
    for n, did in enumerate(DRIVERS_1989):
        for f, v in {"_metadata_": did, "driver": driver_row(did)}.items():
            o.set(f"{dstd}.driver_reference_{n}", f, v)
    o.use("grid")

    o.use("grid_links")
    o.set(f"{GAME_SETUP_MODES}[_metadata_={SETUP_MODE}]", "entrant_setup_modes", SETUP_MODE)
    content = stock.row(CONTENT_SETS, "content_set", CONTENT_SET)
    lists = stock.get(f"{content}.available_entrant_lists_0.entrant_lists").split(",")
    lists += [x for x in (GRID_LIST, CAREER_LIST) if x not in lists]
    o.set(f"{CONTENT_SETS}.content_set[_metadata_={CONTENT_SET}]", "available_entrant_lists_0.entrant_lists",
          ",".join(lists))
    o.use("grid")

    kit_year = stock.row(KIT_YEARS, "team_customisation_for_year", "2026")
    kit_ids = iter(stock.free_ids(kit_year, "team_customisation", 152, 1000))
    kit_template = stock.row(kit_year, "team_customisation", KIT_TEMPLATE)
    for s in slots():
        donor = DONOR_ROW.get(s.team_row, s.team_row)
        o.add(KITS, "team_customisation", kit_template, f"{KITS}.team_customisation[_metadata_={KIT_TEMPLATE}]", {
            **stock.values(stock.row(kit_year, "team_customisation", KIT_ROWS[donor])),
            "_metadata_": f"2026-f1990_{token(s.team)}", "linked_team": team_row(s.team), "suit": SUIT,
            "gloves": SUIT, "stat_id": next(kit_ids),
            "official_driver_customisations": ",".join(looks_of_team.get(s.team, [])),
        }, gated(s.team))

    lead = {s.team: driver_row(regulars[s.team][0].driver) for s in slots()}
    for container, word, sub, child, field in GROUP_TEAM_TABLES:
        group = f"{container}.{word}[_metadata_={DATA_GROUP}].{sub}"
        source = f"{stock.row(container, word, career.DATA_GROUP_SOURCE)}.{sub}"
        for team in EXTRA_TEAMS:
            s = next(x for x in slots() if x.team == team)
            donor = DONOR_ROW[s.team_row]
            values = {field: team_row(team)}
            if child == "TeamModifier":
                values["team_comparison_driver"] = lead[team]
            o.add(group, child, stock.row(source, child, donor, field), f"{group}.{child}[{field}={donor}]", values,
                  gated(team))
        for s in slots():
            if s.team in EXTRA_TEAMS:
                continue
            o.set(f"{group}.{child}[{field}={s.team_row}]", field, team_row(s.team))
            if child == "TeamModifier":
                o.set(f"{group}.{child}[{field}={team_row(s.team)}]", "team_comparison_driver", lead[s.team])

    return o.parts, {"schema_version": 1, "ops": lang}, report


def main(argv):
    if len(argv) < 2:
        sys.exit(__doc__)
    parts, lang, report = build(Stock(argv[1]), Season.load())
    for name, part in parts.items():
        (ROOT / f"db/{name}.json").write_text(json.dumps(part, indent=1, ensure_ascii=False) + "\n",
                                             encoding="utf-8")
    (ROOT / "lang/grid.json").write_text(json.dumps(lang, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(", ".join(f"db/{n}.json: {len(p['ops'])} ops" for n, p in parts.items())
          + f"; lang/grid.json: {len(lang['ops'])} strings")
    if "-v" in argv:
        for did, (label, donor) in report["rows"].items():
            print(f"{did:22} {label:28} from {donor}")


if __name__ == "__main__":
    main(sys.argv)
