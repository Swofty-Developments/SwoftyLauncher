"""The 1990 car table shared by build_cars.py, the vehicle rows and the car pack.

Each team also has a B-spec vehicle `<token>_1990b` in the `f1_1990b` package (folder `f1_1990_vehicle_b_specs`, the
same length as the A folder so the archives rename byte for byte). It shares the A car's folder name, so the same AI
data, plus the A package's common.erp and the A car's handling archive; only the model archive differs. An R&D
milestone swaps a team's entrants from `_1990` to `_1990b` (f1_1990_bspec.dll).

Each 1990 team drives one vehicle row `<team>_1990` in the `f1_1990` vehicle package. Its `asset_filename` is a
2026 car folder, and the model loads from `f1_1990_vehicle_package/teams/<folder>/wep/<folder>.erp`. The package's
`ai_package_folder` is `f1_2026_vehicle_package` in this addon, so the per-track AI data of the 2026 car folder serves
it; the car pack's db/ai.json (tools/ai_data.py) moves it to `AI_PACKAGE_FOLDER`, the 1990 cars' own AI data.

`stem` is the name the car's resources carry inside its archive. It equals the folder, except for the two cars built
on a team donor whose folder is a My Team folder: their resources keep a stem of the donor's length (the archive is
renamed byte for byte), and the archive carries the World, event graph and layers under the folder name as well.
"""
from dataclasses import dataclass

PACKAGE = 'f1_1990'
PACKAGE_FOLDER = 'f1_1990_vehicle_package'
AI_FOLDER = 'f1_2026_vehicle_package'
AI_PACKAGE_FOLDER = PACKAGE_FOLDER
SOURCE_PACKAGE = 'f1_2026_vehicle_package'
SOURCE_PACKAGE_ROW = 'f1_2026'
TOKEN = {'ligier_prost': 'ligier', 'march': 'leytonhouse'}
PACKAGE_B = 'f1_1990b'
PACKAGE_B_FOLDER = 'f1_1990_vehicle_b_specs'


@dataclass(frozen=True)
class Car:
    team: str
    folder: str
    donor: str
    stem: str
    chassis: str
    engine: str
    family: str
    tier: int

    @property
    def token(self):
        return TOKEN.get(self.team, self.team)

    @property
    def vehicle(self):
        return f'{self.token}_1990'

    @property
    def vehicle_b(self):
        return f'{self.token}_1990b'

    @property
    def donor_vehicle(self):
        return f'{self.donor}_2026'


CARS = (
    Car('mclaren', 'mclaren', 'mclaren', 'mclaren', 'McLaren MP4/5B', 'Honda RA100E', 'V10', 0),
    Car('ferrari', 'ferrari', 'ferrari', 'ferrari', 'Ferrari 641', 'Ferrari 036', 'V12', 0),
    Car('williams', 'williams', 'williams', 'williams', 'Williams FW13B', 'Renault RS2', 'V10', 0),
    Car('benetton', 'lotus', 'lotus', 'lotus', 'Benetton B190', 'Ford HBA4', 'V8', 0),
    Car('tyrrell', 'mercedes', 'mercedes', 'mercedes', 'Tyrrell 019', 'Ford DFR', 'V8', 1),
    Car('lotus', 'redbull', 'redbull', 'redbull', 'Lotus 102', 'Lamborghini 3512', 'V12', 1),
    Car('march', 'force_india', 'force_india', 'force_india', 'Leyton House CG901', 'Judd EV', 'V8', 1),
    Car('larrousse', 'myteam_ferrari', 'williams', 'larrouss', 'Lola LC90', 'Lamborghini 3512', 'V12', 2),
    Car('ligier_prost', 'sauber', 'sauber', 'sauber', 'Ligier JS33B', 'Ford DFR', 'V8', 2),
    Car('minardi', 'toro_rosso', 'toro_rosso', 'toro_rosso', 'Minardi M190', 'Ford DFR', 'V8', 1),
    Car('brabham', 'gm_cadillac', 'gm_cadillac', 'gm_cadillac', 'Brabham BT59', 'Judd EV', 'V8', 1),
    Car('dallara', 'myteam_honda', 'haas', 'dall', 'Dallara F190', 'Ford DFR', 'V8', 1),
    Car('arrows', 'haas', 'haas', 'haas', 'Arrows A11B', 'Ford DFR', 'V8', 1),
)

for _c in CARS:
    assert len(_c.stem) == len(_c.donor), _c
assert len(PACKAGE_B_FOLDER) == len(PACKAGE_FOLDER) == len(SOURCE_PACKAGE)


def car(team):
    return next(c for c in CARS if c.team == team)


def group(c):
    return f'asset_groups/{PACKAGE_FOLDER}/teams/{c.folder}/wep/{c.folder}.assetgroup'


def archive(c):
    return f'2025_asset_groups/{PACKAGE_FOLDER}/teams/{c.folder}/wep/{c.folder}.erp'


def group_b(c):
    return f'asset_groups/{PACKAGE_B_FOLDER}/teams/{c.folder}/wep/{c.folder}.assetgroup'


def archive_b(c):
    return f'2025_asset_groups/{PACKAGE_B_FOLDER}/teams/{c.folder}/wep/{c.folder}.erp'


def handling_archive(c):
    return f'2025_asset_groups/{PACKAGE_FOLDER}/teams/{c.folder}/wep/{c.folder}_handling.erp'


def handling_names(c):
    names = [f'eaid://{PACKAGE_FOLDER}/teams/{c.stem}/handling/{c.stem}.vtf']
    if c.stem != c.folder:
        names.append(f'eaid://{PACKAGE_FOLDER}/teams/{c.folder}/handling/{c.folder}.vtf')
    return names


COMMON_ARCHIVE = f'2025_asset_groups/{PACKAGE_FOLDER}/teams/common.erp'
TYRE_GROUP = f'asset_groups/{PACKAGE_FOLDER}/tyrecompounds/tyrecompounds.assetgroup'
TYRE_ARCHIVE = f'2025_asset_groups/{PACKAGE_FOLDER}/tyrecompounds/tyrecompounds.erp'
