#!/usr/bin/env python3
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path

DEFAULT_SOURCE = Path(os.environ.get("F1_1990_SOURCE", "/mnt/work/.f1-1990/source/1990"))

RATINGS = (
    "aggression",
    "avoidance_of_forced_mistakes",
    "avoidance_of_mistakes",
    "blue_flag_conceding",
    "consistency",
    "defending",
    "fuel_management",
    "qualifying_skill",
    "race_skill",
    "stamina",
    "start_reactions",
    "tyre_management",
    "weather_tyre_changes",
    "wet_skill",
)

NATIONALITY = {
    "AUS": "australia",
    "AUT": "austria",
    "BEL": "belgium",
    "BRA": "brazil",
    "CHE": "switzerland",
    "DEU": "germany",
    "FIN": "finland",
    "FRA": "france",
    "GBR": "britain",
    "ITA": "italy",
    "JPN": "japan",
    "SWE": "sweden",
}

NATIONALITY_OVERRIDES = {"martin_donnelly": "northern_ireland"}

TEAMS = (
    ("mclaren", "mclaren_26", "mclaren_2026", "mclaren"),
    ("ferrari", "ferrari_26", "ferrari_2026", "ferrari"),
    ("williams", "williams_26", "williams_2026", "williams"),
    ("benetton", "renault_26", "lotus_2026", "lotus"),
    ("tyrrell", "mercedes_26", "mercedes_2026", "mercedes"),
    ("lotus", "redbull_26", "redbull_2026", "redbull"),
    ("march", "forceindia_26", "force_india_2026", "force_india"),
    ("larrousse", "team12_26", "team12_2026", "williams"),
    ("ligier_prost", "sauber_26", "sauber_2026", "sauber"),
    ("minardi", "tororosso_26", "toro_rosso_2026", "toro_rosso"),
    ("brabham", "gm_cadillac_26", "gm_cadillac_2026", "gm_cadillac"),
    ("dallara", "team13_26", "team13_2026", "haas"),
    ("arrows", "haas_26", "haas_2026", "haas"),
)


@dataclass(frozen=True)
class Race:
    round: int
    name: str
    short_name: str
    date: str
    circuit: str


@dataclass(frozen=True)
class Driver:
    id: str
    name: str
    first: str
    last: str
    nationality: str
    country: str
    birth_year: int
    reputation: str
    ratings: dict


@dataclass(frozen=True)
class Seat:
    driver: str
    number: int
    races: int


@dataclass(frozen=True)
class Team:
    id: str
    name: str
    principal: str
    reputation: str
    colour: str
    prequalifying: bool
    car: str
    power: float
    weight: float
    drag: float
    reliability: float
    seats: tuple
    malus: dict


@dataclass(frozen=True)
class Absence:
    round: int
    team: str
    out: str
    into: str


@dataclass(frozen=True)
class Slot:
    team: str
    team_row: str
    vehicle: str
    asset: str


@dataclass
class Season:
    year: int
    points: dict
    best_results: int
    races: list
    teams: dict
    drivers: dict
    absences: list
    source: Path = field(default=DEFAULT_SOURCE)

    @classmethod
    def load(cls, source=None):
        source = Path(source or DEFAULT_SOURCE)
        season = json.loads((source / "season.json").read_text(encoding="utf-8"))
        roster = json.loads((source / "drivers.json").read_text(encoding="utf-8"))
        races = [
            Race(r["race_id"], r["race_name"], r["race_short_name"], r["race_date"], r["circuit"])
            for r in season["races"]
        ]
        races.sort(key=lambda r: r.round)
        teams = {}
        for t in season["teams"]:
            m = t.get("ams2carPerformanceMalus") or {}
            seats = tuple(
                Seat(c["driver_id"], c["drivernumber"], c["races"])
                for c in (t["driver1_contract"], t["driver2_contract"])
                if c and c.get("driver_id")
            )
            teams[t["team_id"]] = Team(
                id=t["team_id"],
                name=t["team_name"],
                principal=t["team_principal"],
                reputation=t["reputation"],
                colour=t["color"].upper(),
                prequalifying=bool(t["default_prequalifying"]),
                car=t["ams2car"],
                power=-m.get("power_scalar", -1.0),
                weight=-m.get("weight_scalar", -1.0),
                drag=-m.get("drag_scalar", -1.0),
                reliability=-m.get("vehicle_reliability", 0.0),
                seats=seats,
                malus=dict(m),
            )
        drivers = {}
        for d in roster["drivers"]:
            first, _, last = d["name"].partition(" ")
            if d["driver_id"] == "andrea_de_cesaris":
                first, last = "Andrea", "de Cesaris"
            elif d["driver_id"] == "eric_van_de_poele":
                first, last = "Eric", "van de Poele"
            drivers[d["driver_id"]] = Driver(
                id=d["driver_id"],
                name=d["name"],
                first=first,
                last=last,
                nationality=NATIONALITY_OVERRIDES.get(d["driver_id"], NATIONALITY.get(d["nationality"], "default")),
                country=d["nationality"],
                birth_year=d["year_of_birth"],
                reputation=d["reputation"],
                ratings={k: float(d["rating_values"][k]) for k in RATINGS},
            )
        absences = []
        for a in season["absences"]:
            while a:
                absences.append(Absence(a["race_id"], a["teamid"], a["driver_out"], a["driver_in"]))
                a = a.get("chainedAbsence")
        absences.sort(key=lambda a: (a.round, a.team))
        return cls(
            year=season["year"],
            points={int(k): v for k, v in season["points_system"].items()},
            best_results=season["races_to_count_towards_championship"],
            races=races,
            teams=teams,
            drivers=drivers,
            absences=absences,
            source=source,
        )

    def race(self, round):
        return next(r for r in self.races if r.round == round)

    def lineup(self, round=1, teams=None):
        out = {}
        for tid in teams or self.teams:
            team = self.teams[tid]
            seats = []
            for seat in team.seats:
                driver = seat.driver
                for a in self.absences:
                    if a.round == round and a.team == tid and a.out == driver:
                        driver = a.into
                seats.append(Seat(driver, seat.number, seat.races))
            out[tid] = seats
        return out

    def regulars(self, teams=None):
        out = {}
        for tid in teams or self.teams:
            starts = [{} for _ in self.teams[tid].seats]
            for race in self.races:
                for i, seat in enumerate(self.lineup(race.round, [tid])[tid]):
                    starts[i][seat.driver] = starts[i].get(seat.driver, 0) + 1
            out[tid] = [
                Seat(max(c, key=lambda d: (c[d], d == seat.driver)), seat.number, max(c.values()))
                for c, seat in zip(starts, self.teams[tid].seats)
            ]
        return out

    def stand_ins(self, teams=None):
        regular = {s.driver for seats in self.regulars(teams).values() for s in seats}
        out = []
        for race in self.races:
            for tid, seats in self.lineup(race.round, teams).items():
                for seat in seats:
                    if seat.driver not in regular and all(d != seat.driver for d, _ in out):
                        out.append((seat.driver, tid))
        return out

    def substitutions(self, teams=None):
        keep = set(teams or self.teams)
        return [a for a in self.absences if a.team in keep]

    def rating_range(self, name):
        values = [d.ratings[name] for d in self.drivers.values()]
        return min(values), max(values)

    def normalised(self, driver, name):
        lo, hi = self.rating_range(name)
        return 0.5 if hi == lo else (self.drivers[driver].ratings[name] - lo) / (hi - lo)


def slots():
    return [Slot(*t) for t in TEAMS]


def slot(team):
    return next(s for s in slots() if s.team == team or s.team_row == team)


def grid_lineup(season=None, round=None):
    season = season or Season.load()
    teams = [s.team for s in slots()]
    lineup = season.regulars(teams) if round is None else season.lineup(round, teams)
    return [(s, lineup[s.team]) for s in slots()]


def main(argv):
    season = Season.load(argv[1] if len(argv) > 1 else None)
    print(f"{season.year}: {len(season.races)} races, {len(season.teams)} teams, {len(season.drivers)} drivers")
    for s, seats in grid_lineup(season):
        t = season.teams[s.team]
        names = ", ".join(f"#{x.number} {season.drivers[x.driver].name}" for x in seats)
        print(f"{s.team_row:15} {t.name:24} power {t.power:.3f} weight {t.weight:.3f} drag {t.drag:.3f}  {names}")


if __name__ == "__main__":
    main(sys.argv)
