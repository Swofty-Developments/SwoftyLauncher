"""Build the 1990 handling archives, their asset ops and the 1990 car database ops.

Each 1990 team drives its own vehicle row in the `f1_1990` vehicle package (tools/cars.py). Its handling file is built
from the stock 2026 handling file of the car's donor folder and shipped as a small archive,
`f1_1990_vehicle_package/teams/<folder>/wep/<folder>_handling.erp`, one per `ers` and `tyre_size` value. The car pack
(carpack/) lists that archive in the car's asset group next to the car archive and the package's
common.erp. No 2026 archive, row or file is touched.

  EnergyRecoverySystem   removed, and the EnergyRecoverySystem parts of the radUpgrade* blocks
                         (the blocks stay, empty, like the stock radUpgradeWearMGUH)
  Engine                 naturally aspirated (TurboMode Off), the car's engine (ENGINES): the stock 15-vertex
                         RevsPowerSpline grid scaled so vertex 9 (the stock peak, and the peak of the radUpgradeEngine
                         deltas, which add per vertex) sits at the engine's peak power rpm, the power curve of its
                         V8/V10/V12 torque shape through that peak, the limiter LIMITER_MARGIN_RPM (500) above it,
                         RevsEngineBrakingSpline on the same grid, WearICERevsSpline scaled by the limiter ratio
  GearBox                GEARS forward gears (Ferrari 7, every other car 6, as the stock F2 car: GearRatio past the top
                         gear 0, no GearUpSpeedMultiple from the top gear, no GearDnSpeedMultiple above it); 1st keeps
                         the stock 1st gear's speed at the limiter, the top gear (ratio 1.0) reaches the limiter at
                         TOP_SPEED_KMH x TOP_GEAR_MARGIN (1.12, room for a tow), the gears between follow the stock
                         8-speed progression (log spacing) stretched over the N gears; modern keeps the stock top-gear
                         speed; the automatic upshift (a multiple of the speed at peak power in the gear) sits at 72.5 %
                         (1st) and 84 % (2nd) of the limiter, as on the stock car, and at 96 % from 3rd up; the
                         downshift keeps the stock fraction of the limiter in the lower gear at the same place in the
                         box, at least 5 % under that gear's upshift; shift times SHIFT_MANUAL for the H-pattern cars,
                         SHIFT_SEMI_AUTO for the Ferrari 641 (stock 2026 upshift 0.03/0.015/0.045 s)
  FuelTank               BSFC scaled so fuel per second at full power stays stock (baked AI fuel data);
                         FuelOptimalRevs moves with the rev grid
  FuelECU                power scales divided by the stock standard scale, so the team delta is the season pack's
  Chassis                mass from the season pack, MOIX/Y/Z scaled by the same ratio to the donor's mass, MOIX and
                         MOIY also by the square of the 1990 wheelbase over the donor's
  Axles                  the car pack's model sets the wheel positions (UseModelPositions): LocalRestPositionX/Z
                         carry the 1990 model's wheel_fl and wheel_bl nodes (MODEL_AXLES), Chassis COMZ (and
                         F1Tuning ChassisCOMZMin/Max) and FuelTank COMZ keep their fraction of the donor model's
                         wheelbase (DONOR_AXLES), and F1Plank keeps the donor's distance from each axle to the end
                         of the plank (OffsetZ/ExtentZ)
  F1Aero                 drag scaled by the season pack, no moving front wing; downforce stays stock; no DRS
                         hardware: the DRS failure/joint keys removed, flap angle, open/close speed and
                         DRS-open wing factors as on the safety car (0, 1, 1.0), radUpgradeDRS emptied
  R&D ceilings (rnd.py)  radUpgradeEngine power deltas x supplier bhp x 745.7 / 53988; radUpgradeFuelConsumption
                         multipliers at 12/15 (works) or 8/15 (customer) of their stock distance from 1;
                         radUpgradeWeightReduction Mass = the team's weight target

Option tyre_size: 2026 (the default) keeps the donor's tyres (TyreRadius 0.351 front / 0.354 rear; the car pack draws
the 13-inch rim with a tyre of that outer radius) and raises the car by RAISE (30 mm). The wheel's rest position is
the model's (a vector the suspension data never writes) and the body sits at ground + TyreRadius - tyre deflection -
rest Y - height, height being the suspension compression that 0x1446b5f60 solves as MaxHeight - RestLength + load over
the spring rates (the spawn placement 0x144632740 uses the same sum). So each axle's RestLength and the setup's
RestLength range grow by the raise, which lowers the static compression and lifts the body by the raise with the
tyres' load and deflection unchanged; MinHeight, BumpStopHeight and the HeightTo* spline x move down by it so the droop,
the bump-stop margin, the kinematics and the rising-rate third spring keep their place relative to the static car,
MaxHeight stays as it is, and RideHeightOffset, RideHeight and the setup's ride-height range carry the raise, so the
aero ride height (AeroReferencePlaneHeight - (height + RideHeightOffset)) and the downforce stay the donor's. 13in and
13in_raised use TyreRadius
0.323 front / 0.330 rear, AlloyRadius at its 0.175 m bead, the tyre damage node graphs remapped radially from the stock bead and
tread onto the new ones and widened to the new mesh, and the final drive scaled by the rear radius so road speed per
engine rev stays the same. TyreWidth, tyre mass, inertia and compression stay stock. The wheel nodes stay at the 2026
axle height, so the smaller radius alone drops the chassis, floor and plank by the radius drop (2.8 cm front, 2.4 cm
rear). The physics sets the static suspension height from MaxHeight - RestLength and the spring rates (0x1446b5f60) and
ignores RideHeight, so each axle's suspension height frame moves down by the drop plus RAISE instead: MinHeight,
MaxHeight, BumpStopHeight and the x of every HeightTo* spline, with RestLength and the spring rates stock. The car then
sits RAISE above the 2026 ride height with the 2026 travel, bump-stop margin and kinematics. The aero ride height is
AeroReferencePlaneHeight - (height + RideHeightOffset) (0x1446cb000), so RideHeightOffset carries the whole frame move
and the aero maps read the 2026 values. 13in_raised moves the frame by the drop more, lifting the car by RAISE plus the
drop; RideHeight and the setup ride-height range carry the lift.

Kerbs, every variant but stock: BumpStopHeight, MaxHeight and RestLength (and the setup's RestLength range) go up by
KERB_BUMP (15 mm), so the static car keeps its height and has 15 mm more bump travel before the bump stop with the bump
stop's own range kept; MinHeight goes down by KERB_DROOP (10 mm) of extra droop; BumpStopRisingSpring x0.25,
BumpStopRisingDamping x0.5 and FastBumpDamping x0.6 (the fast zone above DampingZoneDivision, 0.06 m/s, so the slow
damping that sets the handling stays). The ConvexHull vertices move like COMZ (each z at the same fraction of the
wheelbase between the donor model's axles and the 1990 model's), so the 2026 hull's floor, nose and wings sit under and
around the shorter 1990 car instead of up to 0.3 m past it.

Option ers: removed and zeroed take the 2026 hybrid out (the section, or every harvest, deploy and override limit at 0);
neither changes mass, ICE power or the battery's weight. stock keeps the donor's EnergyRecoverySystem as it is, with
every other edit above.

Option tyre_size stock (an A/B test value, `ers` ignored): the donor's stock 2026 handling file, renamed into the 1990
package, with only the tyre physics group changed: 2026 tyres, suspension, engine, ERS, mass, aero and DRS, shipped as
cars/handling/stock/<folder>_handling.erp.

Every variant sets Chassis/TyrePhysicsGroup to F1_1990, or F1_1990P for the 1990 Pirelli teams (Tyrrell, Minardi,
Brabham, Dallara); tools/tyres.py writes both groups' compounds. The game builds a wheel's compound name from it
("%s_%s%c": group, physics_code, f or r, lowercased; 0x142ea2300) and looks that up in one global map that every loaded
VCMPD asset fills by compound name. Activation (0x144747c60) skips a name already there and deactivation (0x1447482b0)
erases by name, so two assets sharing names lose them both when either unloads, and a wheel whose name is missing gets
no compound at all. The car pack's compound file names its compounds F1_1990_*, which no stock package uses.

Option handling, one PERIOD entry per value: modern scales MOIX/Y/Z with the mass and keeps the donor's aero, grip
and brakes. 1990 (the default) scales them with the mass like modern; 1990_low keeps the donor's (moi: the share of
the mass scaling kept). Both add the period cuts: every lift curve (F1Aero
Front/RearCoeffLiftVsRideHeight and VsChassisRake) and wing lift coefficient (Front/RearWingCoL0-2) x aero, so the
balance stays the donor's; Tyre GripMultiplier per axle (the tyre force multiplies it in and clamps to
Min/MaxGripMultiplier at 0x1446b7950); BrakeECU torques x the mass ratio x brake; Chassis FFBScale x FFB_1990 (1.3), a
heavier wheel for an unassisted rack; the slipstream of SLIPSTREAM_1990. The game scales each force in a tow by
scale + (1 - scale) x the clean-air share (0x1446dceb0: the m_aeroSlipStreamAirDensityScale* values are the air density
the body drag, each wing and the body lift see in a full tow), so Drag 0.8 -> 0.65 makes the tow 1.75 times the 2026
one at every gap, FrontWing 0.76 -> 0.6 and RearWing 0.82 -> 0.75 take more front than rear downforce (and more wing
drag) in dirty air, Lift 0.91 -> 0.85, and m_aeroSlipStreamCarWidth 1.5 -> 1.25 narrows the wake. They also take period
power and mass: the engine's own power (ENGINES) instead
of BASE_HP 650 x the team scalar, and PERIOD_MASS 620 kg (500 kg dry, a 72 kg driver and 48 kg standing in for the 1990 race fuel the game does not carry)
instead of BASE_MASS 575, and drag x PERIOD_DRAG (0.96) on top of the team scalar. The 2026 donor opens its wings on
the straights (0x1446dceb0 scales the front and rear wing drag by 1 + (m_aero_wing_front_drag - 1) x the open share,
0.5 and 0.3 on the 2026 cars); the 1990 car has no moving wings and keeps the full wing drag, so PERIOD_DRAG 0.96
puts a 690 hp car at about 320 km/h with a Monza wing and a 620 hp car at about 305 (TOP_SPEED_KMH, Swofty AI's car
model with that drag). The
game carries fuel as FuelTank mass on top of Chassis Mass and burns it at the BSFC; the start load follows the race
distance and the per-lap burn the AI has baked per track and car (.fuel_burn_granular), so the BSFC keeps the donor's
fuel per second and the extra period fuel goes into the chassis instead. Archives:
cars/handling/<ers>_<tyre_size>_<handling>/ (modern keeps <ers>_<tyre_size>/).

Team mass and drag (and power under modern) come from the season pack's per-team scalars (season.py), applied around a
common base.

Outputs (relative to the addon folder): cars/handling/<ers>_<tyre_size>/<folder>_handling.erp, cars/handling.json
(file.put ops keyed by the options) and db/cars.json (the f1_1990 and f1_1990b package rows and their car setup
preset rows, the 13 vehicle rows
labelled with the grid lane's tokens and their 13 B-spec copies `<token>_1990b`; the name strings come from the grid
lane's lang ops, `engine` the 1990 engine row), the seven 1990 engine rows `f1990_engine_<supplier>` (copies of
Cosworth_F1_V8_2009-2013 with its rev-light thresholds x the engine's limiter / 18,000) and lang/engines.json (their supplier
names). The game loads a vehicle at startup (0x142505f30, the stat-id indexed vehicle table) only when its
`available` is True, and the content pass 0x1413e4cf0 sets that only for vehicles an entrant list names and for
their packages; no entrant list names a B-spec car, so the B rows and the f1_1990b package carry `available True`. The build compares with the shipped files and exits 1 on any difference; --write replaces them.
"""
import argparse
import itertools
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(ADDON))
for _p in (os.path.join(REPO, 'tools', 'swofty-ui'), os.path.join(REPO, 'tools'), HERE):
    if _p not in sys.path:
        sys.path.insert(0, _p)

import cars as C
import erpw
import gamedir
import rnd
import season
import vtf

GROUPS = '2025_asset_groups'
GAMEDB = 'game_package/gamedatabases/gamedatabases.2025.bin'
PACKAGE = C.SOURCE_PACKAGE
VEHICLE_FIELDS = os.path.join(REPO, 'addons', 'custom-grid-size', 'db', 'grid.json')
# Vehicle keys the custom grid list lacks: every stock 2026 vehicle row has them, and no identifier string in the
# exe names them in full.
VEHICLE_EXTRA_FIELDS = [f'ai_vehicle_config.ers_use.{mode}.push_on_in_or_out_lap_battery_level'
                        for mode in ('xmode_enabled', 'xmode_passive')]

BASE_HP = 650.0
BASE_MASS = 575.0
SPREAD = 1.0
HP_W = 745.7
TORQUE_FLOOR = 0.6

TORQUE_PEAK = {'V12': 0.82, 'V10': 0.78, 'V8': 0.75}
ENGINES = {
    'Honda RA100E': {'hp': 690, 'peak_rpm': 13500, 'supplier': 'honda'},
    'Ferrari 036': {'hp': 680, 'peak_rpm': 12750, 'supplier': 'ferrari'},
    'Renault RS2': {'hp': 660, 'peak_rpm': 12800, 'supplier': 'renault'},
    'Ford HBA4': {'hp': 660, 'peak_rpm': 12800, 'supplier': 'ford_hb'},
    'Judd EV': {'hp': 640, 'peak_rpm': 12500, 'supplier': 'judd'},
    'Lamborghini 3512': {'hp': 640, 'peak_rpm': 13000, 'supplier': 'lamborghini'},
    'Ford DFR': {'hp': 620, 'peak_rpm': 11250, 'supplier': 'ford_dfr'},
}
LIMITER_MARGIN_RPM = 500

ENGINE_TABLE = 'entrants.engines'
ENGINE_TEMPLATE = 'Cosworth_F1_V8_2009-2013'
ENGINE_TEMPLATE_LIMITER = 18000
REV_LIGHTS = {'revLightsStart': 13000, 'revLightsRedStart': 16000, 'revLightsPurpleStart': 17000,
              'revLightWarning': 17800, 'revLightsEnd': 18000}
ENGINE_FIELDS = ['_metadata_', 'nationality', 'revDialAnimEnd', 'revDialEnd', 'revDialStart', 'revFlashSpeed',
                 'revLightWarning', 'revLightsEnd', 'revLightsPurpleStart', 'revLightsRedStart', 'revLightsStart',
                 'stat_id', 'supplier']
ENGINE_ROWS = {
    'honda': ('japan', 'lng_engine_supplier_honda', None),
    'ferrari': ('italy', 'lng_engine_supplier_ferrari', None),
    'renault': ('france', 'lng_engine_supplier_renault', None),
    'ford_hb': ('england', 'lng_f1990_engine_supplier_ford', 'Ford'),
    'ford_dfr': ('england', 'lng_f1990_engine_supplier_ford', 'Ford'),
    'lamborghini': ('italy', 'lng_f1990_engine_supplier_lamborghini', 'Lamborghini'),
    'judd': ('england', 'lng_f1990_engine_supplier_judd', 'Judd'),
}
PEAK_VERTEX = 9

GEAR_NAMES = ('1st', '2nd', '3rd', '4th', '5th', '6th', '7th', '8th', '9th')
STOCK_GEARS = 8
GEARS = {'ferrari': 7}
GEARS_DEFAULT = 6
UPSHIFT_LOW = (0.725, 0.84)
UPSHIFT_HIGH = 0.96
DOWNSHIFT_MARGIN = 0.05
TOP_GEAR_MARGIN = 1.12
TOP_SPEED_KMH = {
    'mclaren': 321, 'ferrari': 317, 'williams': 315, 'lotus': 315, 'mercedes': 308, 'redbull': 311,
    'force_india': 311, 'myteam_ferrari': 311, 'sauber': 308, 'toro_rosso': 306, 'gm_cadillac': 309,
    'myteam_honda': 309, 'haas': 307,
}
SHIFT_MANUAL = {b'UpshiftClutchTime': 0.1, b'UpshiftGearDelay': 0.05, b'TimeOffThrottleGearUp': 0.1,
                b'DownshiftClutchTime': 0.12, b'DownshiftGearDelay': 0.06, b'TimeOnThrottleGearDown': 0.1}
SHIFT_SEMI_AUTO = {b'UpshiftClutchTime': 0.06, b'UpshiftGearDelay': 0.03, b'TimeOffThrottleGearUp': 0.06,
                   b'DownshiftClutchTime': 0.075, b'DownshiftGearDelay': 0.03, b'TimeOnThrottleGearDown': 0.07}

MODEL_AXLES = {
    'mclaren': (1.4672, -1.4664, 0.8924, 0.8325), 'sauber': (1.4672, -1.4664, 0.8924, 0.8325),
    'ferrari': (1.4323, -1.4325, 0.8924, 0.8327), 'myteam_ferrari': (1.4323, -1.4325, 0.8924, 0.8327),
    'haas': (1.4323, -1.4325, 0.8924, 0.8327), 'lotus': (1.4403, -1.4421, 0.8279, 0.8014),
    'mercedes': (1.4549, -1.4549, 0.9102, 0.8309), 'force_india': (1.4549, -1.4549, 0.9102, 0.8309),
    'toro_rosso': (1.4549, -1.4549, 0.9102, 0.8309), 'redbull': (1.1827, -1.5750, 0.8279, 0.7951),
    'gm_cadillac': (1.1827, -1.5750, 0.8279, 0.7951), 'williams': (1.1582, -1.6310, 0.8257, 0.7973),
    'myteam_honda': (1.1582, -1.6310, 0.8257, 0.7973),
}
DONOR_AXLES = {
    'mclaren': (1.6656, -1.5925), 'ferrari': (1.6991, -1.6991), 'williams': (1.6876, -1.6876),
    'lotus': (1.6999, -1.6999), 'mercedes': (1.6951, -1.7052), 'redbull': (1.7014, -1.7014),
    'force_india': (1.6995, -1.6993), 'sauber': (1.6733, -1.6943), 'toro_rosso': (1.6945, -1.6999),
    'gm_cadillac': (1.7001, -1.7000), 'haas': (1.6930, -1.6930),
}
PITCH_YAW = (b'MOIX', b'MOIY')

PACKAGE_FIELDS = ['_metadata_', 'ai_package_folder', 'available', 'blob_shadow_decal', 'crew_livery_asset_group',
                  'driver_animation_set', 'garage_asset_group', 'has_cockpit_halo', 'is_26_aero_regulations',
                  'is_equal_performance_vehicle_package', 'is_story_vehicle_package', 'pit_stop_asset_group',
                  'pit_wall_asset_group', 'resource_path', 'resource_path_specialisation', 'stat_id',
                  'tyre_compounds_asset_group'] + [
    f'ai_vehicle_package_config.driver_parameter_maps.{name}.{field}'
    for name in ('alongside_awareness', 'alongside_space_factor', 'braking_distance_factor', 'brake_off_time',
                 'corner_curvature_factor', 'follow_speed_factor', 'slide_confidence', 'throttle_confidence',
                 'throttle_exit_fallibility', 'throttle_on_time', 'wet_weather_ability')
    for field in ('characteristic', 'high_difficulty_high_characteristic_mapping',
                  'high_difficulty_low_characteristic_mapping', 'low_difficulty_high_characteristic_mapping',
                  'low_difficulty_low_characteristic_mapping')]
PACKAGE_SET = {
    'ai_package_folder': C.AI_FOLDER,
    'tyre_compounds_asset_group': f'{C.PACKAGE_FOLDER}/tyrecompounds/tyrecompounds',
    'blob_shadow_decal': f'eaid://{C.PACKAGE_FOLDER}/shared/ao/materials/blobshadowdecal.material',
}

ERS_UPGRADES = [b'radUpgradeERS', b'radUpgradeWearES', b'radUpgradeWearMGUK']
DRS_REMOVED = [b'DistanceDrivenCumulativeDRSFailureChance', b'DRSRotationJointAngleRatio', b'DRSFailTimeMin',
               b'DRSFailTimeMax']
DRS_SET = {b'DRSRotationAngleRadians': '0.0', b'DRSOpenCloseSpeed': '1', b'm_aero_wing_front_drag': '1.0',
           b'm_aero_wing_front_force': '1.0', b'm_aero_wing_rear_drag': '1.0', b'm_aero_wing_rear_force': '1.0'}
ERS_ZERO = {b'MaxDeployPower': 0, b'MaxDeratePower': 0, b'MinInitialDeployPower': 0, b'HarvestLimitMGUK': 0,
            b'HarvestOverrideBonusMGUK': 0, b'HarvestLimitQualifyingMGUK': 0, b'DeployLimitMGUK': 0,
            b'MaxBrakeTorqueMGUK': 0, b'OverrideModeAvailable': 'false'}
ERS_VALUES = ('removed', 'zeroed', 'stock')
TYRE_SIZES = ('2026', '13in', '13in_raised')
TYRES_13 = {'Front': {'radius': 0.323, 'width': 0.311}, 'Rear': {'radius': 0.330, 'width': 0.454}}
STOCK_MESH_WIDTH = {'Front': 0.339, 'Rear': 0.418}
BEAD_13 = 0.175
ALLOY_13 = 0.182
TYRE_GROUP = 'F1_1990'
PIRELLI_GROUP = 'F1_1990P'
PIRELLI_TEAMS = ('tyrrell', 'minardi', 'brabham', 'dallara')
MOMENTS = (b'MOIX', b'MOIY', b'MOIZ')
PERIOD_DRAG = 0.96
PERIOD_MASS = 500.0 + 72.0 + 48.0
PERIOD = {
    '1990': {'aero': 0.5, 'grip': {'Front': 0.915, 'Rear': 0.925}, 'brake': 1.0, 'moi': 1.0},
    '1990_low': {'aero': 0.33, 'grip': {'Front': 0.83, 'Rear': 0.85}, 'brake': 0.9, 'moi': 0.0},
    'modern': None,
}
HANDLING_VALUES = tuple(PERIOD)
SLIPSTREAM_1990 = {
    b'm_aeroSlipStreamAirDensityScaleDrag': (0.8, 0.65),
    b'm_aeroSlipStreamAirDensityScaleFrontWing': (0.76, 0.6),
    b'm_aeroSlipStreamAirDensityScaleRearWing': (0.82, 0.75),
    b'm_aeroSlipStreamAirDensityScaleLift': (0.91, 0.85),
    b'm_aeroSlipStreamCarWidth': (1.5, 1.25),
}
FFB_1990 = 1.3
LIFT_CURVES = (b'FrontCoeffLiftVsRideHeight', b'RearCoeffLiftVsRideHeight', b'FrontCoeffLiftVsChassisRake',
               b'RearCoeffLiftVsChassisRake')
LIFT_WINGS = (b'FrontWingCoL0', b'FrontWingCoL1', b'FrontWingCoL2', b'RearWingCoL0', b'RearWingCoL1', b'RearWingCoL2')
BRAKES = (b'BrakeFront', b'BrakeRear', b'BrakeFrontBackwards', b'BrakeRearBackwards', b'BrakeFrontForwards',
          b'BrakeRearForwards')
TYRE_GRAPHS = {b'x0_tyre_fl': 'Front', b'x0_tyre_fr': 'Front', b'x0_tyre_bl': 'Rear', b'x0_tyre_br': 'Rear'}
SUSPENSION_HEIGHTS = (b'MinHeight', b'MaxHeight', b'BumpStopHeight')
RAISE_HEIGHTS = (b'MinHeight', b'BumpStopHeight')
RAISE = 0.030
KERB_BUMP = 0.015
KERB_DROOP = 0.010
KERB_SCALE = {b'BumpStopRisingSpring': 0.25, b'BumpStopRisingDamping': 0.5, b'FastBumpDamping': 0.6}


RND_ENGINE_PEAK = 53988
RND_FUEL_WORKS = 12 / 15
RND_FUEL_CUSTOMER = 8 / 15
RND_EXPECTED = {
    'mclaren': (1.036, 26.4), 'ferrari': (0.994, 12.0), 'williams': (0.912, 33.0), 'benetton': (0.746, 31.8),
    'lotus': (0.58, 33.6), 'larrousse': (0.58, 33.6), 'march': (0.456, 40.0), 'brabham': (0.456, 40.0),
    'tyrrell': (0.331, 40.0), 'ligier_prost': (0.331, 40.0), 'minardi': (0.331, 40.0), 'dallara': (0.331, 40.0),
    'arrows': (0.331, 40.0),
}


def game_root(game):
    if game and os.path.isdir(os.path.join(game, GROUPS, PACKAGE)) and os.path.isfile(os.path.join(game, GAMEDB)):
        return game
    try:
        ui = gamedir.resolve(game)[0]
    except gamedir.NotFound as ex:
        sys.exit(f'{ex}: pass --game')
    return os.path.dirname(os.path.dirname(ui))


def rnd_targets(c):
    token = next(k for k, v in rnd.TOKENS.items() if c.token in v[0])
    teams, supplier, works, _ = rnd.TOKENS[token]
    k = rnd.SUPPLIER_BHP[supplier] * HP_W / RND_ENGINE_PEAK
    got = (round(k, 3), round(-rnd.weight_target(teams[0]), 1))
    if got != RND_EXPECTED[c.team]:
        raise ValueError(f'{c.team}: R&D targets {got}, expected {RND_EXPECTED[c.team]}')
    return {'engine_scale': k, 'fuel_share': RND_FUEL_WORKS if works else RND_FUEL_CUSTOMER,
            'weight': -rnd.weight_target(teams[0])}


def engine(c):
    e = ENGINES[c.engine]
    token = next(k for k, v in rnd.TOKENS.items() if c.token in v[0])
    if rnd.TOKENS[token][1] != e['supplier']:
        raise ValueError(f'{c.team}: engine {c.engine} is {e["supplier"]}, rnd.py says {rnd.TOKENS[token][1]}')
    return {'name': c.engine, 'family': c.family, 'hp': e['hp'], 'peak_rpm': e['peak_rpm'],
            'limiter_rpm': e['peak_rpm'] + LIMITER_MARGIN_RPM, 'torque_peak': TORQUE_PEAK[c.family]}


def engine_rows():
    rows = {}
    for name, e in ENGINES.items():
        limiter = e['peak_rpm'] + LIMITER_MARGIN_RPM
        nationality, supplier, _ = ENGINE_ROWS[e['supplier']]
        lights = {k: str(round(v * limiter / ENGINE_TEMPLATE_LIMITER / 10) * 10) for k, v in REV_LIGHTS.items()}
        rows[e['supplier']] = {'_metadata_': rnd.engine_label(e['supplier']), 'nationality': nationality,
                               'supplier': supplier, **lights}
    return rows


def engine_lang():
    keys = {}
    for nationality, key, text in ENGINE_ROWS.values():
        if text:
            keys[key] = text
    return {'schema_version': 1,
            'ops': [{'op': 'lang.set', 'language': '*', 'key': k, 'value': v} for k, v in sorted(keys.items())]}


def cars():
    pack = season.Season.load()
    rows = []
    for c in C.CARS:
        t = pack.teams[c.team]
        rows.append({'car': c, 'name': t.name, 'engine': engine(c), 'power': t.power, 'weight': t.weight,
                     'drag': t.drag, 'rnd': rnd_targets(c)})
    return rows


def upgrade(root, name):
    hits = [c for c in root.children if c.kind == 0 and c.name == b'Upgrade' and c.attr(b'name') == name]
    if len(hits) != 1:
        raise KeyError(f'{name.decode()}: {len(hits)} Upgrade blocks')
    return hits[0]


def leaves(node):
    for c in node.children:
        if c.kind != 0:
            continue
        if c.attr(b'value') is not None:
            yield c
        else:
            yield from leaves(c)


def rnd_upgrades(root, targets):
    curve = child(child(upgrade(root, b'radUpgradeEngine'), b'Engine'), b'RevsPowerSpline')
    points = [e for e in curve.children if e.kind == 0]
    if max(float(e.attr(b'y')) for e in points) != RND_ENGINE_PEAK:
        raise ValueError('radUpgradeEngine peak is not the stock 53988 W')
    for e in points:
        e.set_attr(b'y', fmt(round(float(e.attr(b'y')) * targets['engine_scale'])))
    for leaf in leaves(upgrade(root, b'radUpgradeFuelConsumption')):
        if leaf.attr(b'op') != b'*':
            raise ValueError(f'radUpgradeFuelConsumption {leaf.name.decode()} is not a multiplier')
        v = float(leaf.attr(b'value'))
        leaf.set_attr(b'value', fmt(round(1 + (v - 1) * targets['fuel_share'], 6)))
    mass = child(child(upgrade(root, b'radUpgradeWeightReduction'), b'Chassis'), b'Mass')
    if mass.attr(b'op') != b'-':
        raise ValueError('radUpgradeWeightReduction Mass is not a subtraction')
    mass.set_attr(b'value', fmt(round(targets['weight'], 1)))


def spread(s):
    return 1.0 + (s - 1.0) * SPREAD


def rpm(x):
    return x * 60.0 / (2.0 * math.pi)


def rads(r):
    return r * 2.0 * math.pi / 60.0


def power_fraction(eng, omega):
    peak = rads(eng['peak_rpm'])
    rt = eng['torque_peak']
    k = 1.0 / ((1.0 - rt) ** 2 + 2.0 * (1.0 - rt))

    def torque(r):
        return max(1.0 - k * (r - rt) ** 2, TORQUE_FLOOR)

    r = omega / peak
    return r * torque(r) / torque(1.0)


def child(node, name):
    hits = [c for c in node.children if c.kind == 0 and c.name == name]
    if len(hits) != 1:
        raise KeyError(f'{name.decode()}: {len(hits)} matches under {node.name.decode()}')
    return hits[0]


def section(root, name):
    return child(root, name)


def value(node, name):
    return float(child(node, name).attr(b'value'))


def set_value(node, name, v):
    child(node, name).set_attr(b'value', fmt(v))


def fmt(v):
    if isinstance(v, str):
        return v.encode()
    s = f'{v:.6g}'
    if 'e' in s:
        s = f'{v:.9f}'.rstrip('0').rstrip('.')
    return s.encode()


def spline(node):
    return [(float(e.attr(b'x')), float(e.attr(b'y'))) for e in node.children if e.kind == 0]


def remove_ers(root):
    removed = [c for c in root.children if c.kind == 0 and c.name == b'EnergyRecoverySystem']
    if len(removed) != 1:
        raise ValueError('expected one EnergyRecoverySystem section')
    root.children = [c for c in root.children if c not in removed]
    for c in root.children:
        if c.kind == 0 and c.name == b'Upgrade':
            c.children = [k for k in c.children if not (k.kind == 0 and k.name == b'EnergyRecoverySystem')]
    for c in root.children:
        if c.kind == 0 and c.name == b'Upgrade' and c.attr(b'name') in ERS_UPGRADES:
            if any(k.kind == 0 for k in c.children):
                raise ValueError(f'{c.attr(b"name").decode()} keeps non-ERS parts')


def zero_ers(root):
    common = child(section(root, b'EnergyRecoverySystem'), b'Common')
    for k, v in ERS_ZERO.items():
        set_value(common, k, v)


def keep_ers(root):
    pass


ERS_MODES = {'removed': remove_ers, 'zeroed': zero_ers, 'stock': keep_ers}


def remove_drs(root, aero):
    aero.children = [k for k in aero.children if not (k.kind == 0 and k.name in DRS_REMOVED)]
    for k, v in DRS_SET.items():
        set_value(aero, k, v)
    for c in root.children:
        if c.kind == 0 and c.name == b'Upgrade' and c.attr(b'name') == b'radUpgradeDRS':
            c.children = [k for k in c.children if k.kind != 0]


def named(node, name):
    return {c.attr(b'name').decode(): c for c in node.children if c.kind == 0 and c.name == name}


def remap_graph(graph, end, stock_radius):
    nodes = [n for n in graph.children if n.kind == 0 and n.name == b'node']
    radii = [math.hypot(float(n.attr(b'y')), float(n.attr(b'z'))) for n in nodes]
    bead, outer = min(radii), max(radii)
    new_outer = TYRES_13[end]['radius'] * outer / stock_radius
    widen = TYRES_13[end]['width'] / STOCK_MESH_WIDTH[end]
    for n, r in zip(nodes, radii):
        k = (BEAD_13 + (r - bead) * (new_outer - BEAD_13) / (outer - bead)) / r
        n.set_attr(b'x', fmt(float(n.attr(b'x')) * widen))
        n.set_attr(b'y', fmt(float(n.attr(b'y')) * k))
        n.set_attr(b'z', fmt(float(n.attr(b'z')) * k))


def shift_heights(node, d, keys=SUSPENSION_HEIGHTS):
    for k in keys:
        set_value(node, k, round(value(node, k) - d, 4))
    for c in node.children:
        if c.kind == 0 and c.name.startswith(b'HeightTo'):
            for e in c.children:
                if e.kind == 0:
                    e.set_attr(b'x', fmt(round(float(e.attr(b'x')) - d, 6)))


def tyres(root, size):
    susp = section(root, b'SuspensionSystem')
    if size == '2026':
        tuning = section(root, b'F1Tuning')
        for end, node in named(susp, b'Suspension').items():
            set_value(node, b'RestLength', round(value(node, b'RestLength') + RAISE, 4))
            shift_heights(node, RAISE, RAISE_HEIGHTS)
            for k in (b'RideHeightOffset', b'RideHeight'):
                set_value(node, k, round(value(node, k) + RAISE, 4))
            for k in (f'{end}SuspensionRestLengthMin', f'{end}SuspensionRestLengthMax', f'{end}RideHeightMin',
                      f'{end}RideHeightMax'):
                set_value(tuning, k.encode(), round(value(tuning, k.encode()) + RAISE, 4))
        return 1.0
    tyre = named(susp, b'Tyre')
    stock = {end: value(tyre[end], b'TyreRadius') for end in TYRES_13}
    for end, t in TYRES_13.items():
        set_value(tyre[end], b'TyreRadius', t['radius'])
        set_value(tyre[end], b'AlloyRadius', ALLOY_13)
    for graph in section(root, b'F1TyreDamage').children:
        if graph.kind == 0 and graph.name == b'PhysicsNodeGraph':
            end = TYRE_GRAPHS[graph.attr(b'name')]
            remap_graph(graph, end, stock[end])
    tuning = section(root, b'F1Tuning')
    for end, node in named(susp, b'Suspension').items():
        drop = stock[end] - TYRES_13[end]['radius']
        lift = RAISE + (drop if size == '13in_raised' else 0.0)
        shift_heights(node, drop + lift)
        set_value(node, b'RideHeightOffset', round(value(node, b'RideHeightOffset') + drop + lift, 4))
        set_value(node, b'RideHeight', round(value(node, b'RideHeight') + lift, 4))
        for k in (f'{end}RideHeightMin', f'{end}RideHeightMax'):
            set_value(tuning, k.encode(), round(value(tuning, k.encode()) + lift, 4))
    return TYRES_13['Rear']['radius'] / stock['Rear']


def kerbs(root):
    tuning = section(root, b'F1Tuning')
    for end, node in named(section(root, b'SuspensionSystem'), b'Suspension').items():
        for k in (b'BumpStopHeight', b'MaxHeight', b'RestLength'):
            set_value(node, k, round(value(node, k) + KERB_BUMP, 4))
        for k in (f'{end}SuspensionRestLengthMin', f'{end}SuspensionRestLengthMax'):
            set_value(tuning, k.encode(), round(value(tuning, k.encode()) + KERB_BUMP, 4))
        set_value(node, b'MinHeight', round(value(node, b'MinHeight') - KERB_DROOP, 4))
        for k, f in KERB_SCALE.items():
            set_value(node, k, round(value(node, k) * f))


def tyre_group(root, c):
    group = PIRELLI_GROUP if c.team in PIRELLI_TEAMS else TYRE_GROUP
    set_value(section(root, b'Chassis'), b'TyrePhysicsGroup', group)


def period(root, ratio, cut):
    aero = section(root, b'F1Aero')
    for name in LIFT_CURVES:
        for e in child(aero, name).children:
            if e.kind == 0:
                e.set_attr(b'y', fmt(round(float(e.attr(b'y')) * cut['aero'], 6)))
    for k in LIFT_WINGS:
        set_value(aero, k, value(aero, k) * cut['aero'])
    for end, t in named(section(root, b'SuspensionSystem'), b'Tyre').items():
        set_value(t, b'GripMultiplier', cut['grip'][end])
    brakes = section(root, b'BrakeECU')
    for k in BRAKES:
        set_value(brakes, k, round(value(brakes, k) * ratio * cut['brake']))
    chassis = section(root, b'Chassis')
    set_value(chassis, b'FFBScale', value(chassis, b'FFBScale') * FFB_1990)
    for k, (stock, v) in SLIPSTREAM_1990.items():
        if value(aero, k) != stock:
            raise ValueError(f'{k.decode()} is {value(aero, k)}, expected the stock {stock}')
        set_value(aero, k, v)


def scale_x(node, k):
    for e in node.children:
        if e.kind == 0:
            e.set_attr(b'x', fmt(round(float(e.attr(b'x')) * k, 1)))


def axle_frame(c):
    zf, zr, xf, xr = MODEL_AXLES[c.folder]
    df, dr = DONOR_AXLES[c.donor]
    return zf, zr, xf, xr, df, dr


def geometry(root, c):
    zf, zr, xf, xr, df, dr = axle_frame(c)

    def place(z):
        return round(zr + (z - dr) / (df - dr) * (zf - zr), 4)

    for part in section(root, b'ConvexHull').children:
        if part.kind == 0 and part.name == b'ConvexHullData':
            for v in part.children:
                if v.kind == 0:
                    v.set_attr(b'z', fmt(place(float(v.attr(b'z')))))

    chassis = section(root, b'Chassis')
    comz = place(value(chassis, b'COMZ'))
    set_value(chassis, b'COMZ', comz)
    tuning = section(root, b'F1Tuning')
    for k in (b'ChassisCOMZMin', b'ChassisCOMZMax'):
        set_value(tuning, k, comz)
    tank = section(root, b'FuelTank')
    set_value(tank, b'COMZ', place(value(tank, b'COMZ')))
    for end, node in named(section(root, b'SuspensionSystem'), b'Suspension').items():
        set_value(node, b'LocalRestPositionZ', zf if end == 'Front' else zr)
        set_value(node, b'LocalRestPositionX', xf if end == 'Front' else xr)
    plank = section(root, b'F1Plank')
    off, ext = value(plank, b'OffsetZ'), value(plank, b'ExtentZ')
    front = zf - (df - (off + ext))
    rear = zr + ((off - ext) - dr)
    set_value(plank, b'OffsetZ', round((front + rear) / 2, 3))
    set_value(plank, b'ExtentZ', round((front - rear) / 2, 3))
    return ((zf - zr) / (df - dr)) ** 2


def engine_curve(root, eng_row, peak_w):
    eng = section(root, b'Engine')
    curve = child(eng, b'RevsPowerSpline')
    stock_curve = spline(curve)
    braking = child(eng, b'RevsEngineBrakingSpline')
    if [x for x, _ in spline(braking)] != [x for x, _ in stock_curve]:
        raise ValueError('RevsEngineBrakingSpline and RevsPowerSpline grids differ')
    stock_max = max(y for _, y in stock_curve)
    if stock_curve[PEAK_VERTEX][1] < stock_max * 0.999:
        raise ValueError('the stock power peak is not at vertex 9')
    k = rads(eng_row['peak_rpm']) / stock_curve[PEAK_VERTEX][0]
    for e in [e for e in curve.children if e.kind == 0]:
        x = float(e.attr(b'x')) * k
        e.set_attr(b'x', fmt(round(x, 1)))
        e.set_attr(b'y', fmt(round(peak_w * power_fraction(eng_row, x))))
    scale_x(braking, k)
    stock_limiter = value(eng, b'LimiterRotationRate')
    limiter = rads(eng_row['limiter_rpm'])
    set_value(eng, b'LimiterRotationRate', round(limiter, 1))
    scale_x(child(eng, b'WearICERevsSpline'), limiter / stock_limiter)
    set_value(eng, b'TurboMode', 'Off')
    tank = section(root, b'FuelTank')
    set_value(tank, b'FuelOptimalRevs', round(value(tank, b'FuelOptimalRevs') * k, 1))
    new_curve = spline(curve)
    return {'stock_curve': stock_curve, 'stock_peak': stock_max, 'stock_limiter': stock_limiter,
            'limiter': limiter, 'peak_x': new_curve[PEAK_VERTEX][0],
            'real_peak': max(y for x, y in new_curve if x <= limiter)}


def interp(xs, ys, x):
    for i in range(len(xs) - 1):
        if x <= xs[i + 1]:
            return ys[i] + (ys[i + 1] - ys[i]) * (x - xs[i]) / (xs[i + 1] - xs[i])
    return ys[-1]


def spacing(stock, n):
    span = math.log(stock[0] / stock[-1])
    steps = [i * (len(stock) - 1) / (n - 1) for i in range(n)]
    share = [math.log(stock[0] / r) / span for r in stock]
    return [interp(list(range(len(stock))), share, x) for x in steps]


def gear_count(c):
    return GEARS.get(c.team, GEARS_DEFAULT)


def gearbox(root, c, e, rolling, top_kmh):
    box = section(root, b'GearBox')
    n = gear_count(c)
    limiter, stock_limiter = e['limiter'], e['stock_limiter']
    stock = [value(box, f'GearRatio{g}'.encode()) for g in GEAR_NAMES[:STOCK_GEARS]]
    if len(stock) != STOCK_GEARS or stock[-1] != 1.0:
        raise ValueError(f'{c.team}: expected 8 stock ratios ending at 1.0, got {stock}')
    final = value(box, b'FinalDriveRatio') * limiter / stock_limiter * rolling
    first = stock[0] * final
    if top_kmh is not None:
        radius = value(named(section(root, b'SuspensionSystem'), b'Tyre')['Rear'], b'TyreRadius')
        final = limiter * radius / (top_kmh / 3.6)
    final = round(final, 3)
    span = math.log(first / final)
    ratios = [round(math.exp(span * (1 - u)), 3) for u in spacing(stock, n)]
    set_value(box, b'FinalDriveRatio', final)
    for i, g in enumerate(GEAR_NAMES[:STOCK_GEARS]):
        set_value(box, f'GearRatio{g}'.encode(), ratios[i] if i < n else 0)

    stock_peak_x = max(e['stock_curve'], key=lambda p: p[1])[0]
    up_stock = [value(box, f'GearUpSpeedMultiple{g}'.encode()) * stock_peak_x / stock_limiter
                for g in GEAR_NAMES[:STOCK_GEARS - 1]]
    dn_stock = [value(box, f'GearDnSpeedMultiple{g}'.encode()) * stock_peak_x / stock_limiter
                for g in GEAR_NAMES[1:STOCK_GEARS]]
    up = [UPSHIFT_LOW[k] if k < len(UPSHIFT_LOW) else UPSHIFT_HIGH for k in range(n - 1)]
    places = [k * (len(dn_stock) - 1) / (n - 2) for k in range(n - 1)]
    dn = [min(interp(list(range(len(dn_stock))), dn_stock, x), up[k] - DOWNSHIFT_MARGIN) for k, x in enumerate(places)]
    if max(up) >= 1.0 or any(d >= u for d, u in zip(dn, up)) or not up_stock:
        raise ValueError(f'{c.team}: shift points {up} / {dn} not below the limiter')
    k = limiter / e['peak_x']
    keep = set()
    for i, f in enumerate(up):
        name = f'GearUpSpeedMultiple{GEAR_NAMES[i]}'.encode()
        set_value(box, name, round(f * k, 3))
        keep.add(name)
    for i, f in enumerate(dn):
        name = f'GearDnSpeedMultiple{GEAR_NAMES[i + 1]}'.encode()
        set_value(box, name, round(f * k, 3))
        keep.add(name)
    box.children = [g for g in box.children if not (
        g.kind == 0 and g.name.startswith((b'GearUpSpeedMultiple', b'GearDnSpeedMultiple')) and g.name not in keep)]
    for name, v in (SHIFT_SEMI_AUTO if n > GEARS_DEFAULT else SHIFT_MANUAL).items():
        set_value(box, name, v)
    return final, n


def convert(root, row, ers, size, handling):
    c = row['car']
    eng_row = row['engine']
    cut = PERIOD[handling]
    tyre_group(root, c)
    ERS_MODES[ers](root)
    rolling = tyres(root, size)
    kerbs(root)

    peak_w = (BASE_HP * spread(row['power']) if cut is None else eng_row['hp']) * HP_W
    e = engine_curve(root, eng_row, peak_w)
    limiter, stock_limiter = e['limiter'], e['stock_limiter']

    top_kmh = None if cut is None else TOP_SPEED_KMH[c.folder] * TOP_GEAR_MARGIN
    final, gears = gearbox(root, c, e, rolling, top_kmh)

    real_peak = e['real_peak']
    tank = section(root, b'FuelTank')
    for k in (b'FuelBsfcMin', b'FuelBsfcMax'):
        set_value(tank, k, value(tank, k) * e['stock_peak'] / real_peak)

    ecu = section(root, b'FuelECU')
    std = value(ecu, b'FuelPowerScaleStandard')
    for k in (b'FuelPowerScaleLean', b'FuelPowerScaleStandard', b'FuelPowerScaleRich', b'FuelPowerScaleMax'):
        set_value(ecu, k, round(value(ecu, k) / std, 4))

    length = geometry(root, c)
    chassis = section(root, b'Chassis')
    base_mass = BASE_MASS if cut is None else PERIOD_MASS
    mass = base_mass * spread(row['weight'])
    ratio = mass / value(chassis, b'Mass')
    set_value(chassis, b'Mass', round(mass, 1))
    moi = 1.0 if cut is None else cut['moi']
    for k in MOMENTS:
        size_k = length if k in PITCH_YAW else 1.0
        set_value(chassis, k, round(value(chassis, k) * (1 + (ratio - 1) * moi) * size_k, 1))
    if cut is not None:
        period(root, ratio, cut)

    aero = section(root, b'F1Aero')
    drag = spread(row['drag']) * (1.0 if cut is None else PERIOD_DRAG)
    for k in (b'BodyDragCoeffMin', b'BodyDragCoeffMax', b'FrontWingCoD0', b'FrontWingCoD1',
              b'RearWingCoD0', b'RearWingCoD1'):
        set_value(aero, k, value(aero, k) * drag)
    set_value(aero, b'FrontWingRotationAngleRadians', 0)
    remove_drs(root, aero)
    rnd_upgrades(root, row['rnd'])

    plank = section(root, b'F1Plank')
    return {'team': row['name'], 'engine': eng_row['name'], 'hp': round(real_peak / HP_W),
            'peak_rpm': eng_row['peak_rpm'], 'limiter_rpm': eng_row['limiter_rpm'], 'mass': round(mass, 1),
            'drag': round(drag, 3), 'final_drive': round(final, 3), 'gears': gears, 'comz': value(chassis, b'COMZ'),
            'moi': tuple(value(chassis, k) for k in MOMENTS),
            'plank': (value(plank, b'OffsetZ'), value(plank, b'ExtentZ'))}


def stock_archive(root_dir, c):
    return os.path.join(root_dir, GROUPS, PACKAGE, 'teams', c.donor, 'wep', f'{c.donor}.erp')


def stock_vtf(root_dir, c):
    e = erpw.Erp(stock_archive(root_dir, c))
    r = e.find(f'eaid://{PACKAGE}/teams/{c.donor}/handling/{c.donor}.vtf')
    return e, r


def handling_archive(e, r, names, f0, f1):
    out = []
    for name in names:
        q = erpw.Res()
        q.name, q.type, q.unk, q.hash, q.frags = name, r.type, r.unk, r.hash, []
        for fr in r.frags:
            g = erpw.Frag()
            g.name, g.off, g.size, g.flags, g.comp, g.packed, g.raw = (fr.name, fr.off, fr.size, fr.flags, fr.comp,
                                                                       fr.packed, fr.raw)
            q.frags.append(g)
        out.append(q)
    e.res = out
    for q in out:
        e.set_frag(q, 0, f0)
        e.set_frag(q, 1, f1)
    tmp = os.path.join(ADDON, 'build', '.handling.erp')
    os.makedirs(os.path.dirname(tmp), exist_ok=True)
    e.to_file(tmp)
    with open(tmp, 'rb') as f:
        blob = f.read()
    os.remove(tmp)
    return blob


def build(game, out):
    root_dir = game_root(game)
    rows = cars()
    files, ops, report = {}, [], []
    for row in rows:
        c = row['car']
        e, r = stock_vtf(root_dir, c)
        f0, f1 = e.frag_data(r, 0), e.frag_data(r, 1)
        stock = vtf.parse(f0)
        version = int.from_bytes(f1[4:8], 'little')
        if vtf.write(stock) != f0 or vtf.tsmd(stock, version) != f1:
            raise ValueError(f'{c.team}: stock VTF of {c.donor} does not round-trip')
        for ers, tyre_size, handling in itertools.product(ERS_VALUES, TYRE_SIZES, HANDLING_VALUES):
            root = vtf.parse(f0)
            got = convert(root, row, ers, tyre_size, handling)
            n0 = vtf.write(root)
            n1 = vtf.tsmd(root, version)
            if (ers, tyre_size, handling) == ('removed', '2026', '1990'):
                info, size = got, len(n0)
            suffix = '' if handling == 'modern' else f'_{handling}'
            rel = f'cars/handling/{ers}_{tyre_size}{suffix}/{c.folder}_handling.erp'
            files[rel] = handling_archive(e, r, C.handling_names(c), n0, n1)
            ops.append({'op': 'file.put', 'when': {'ers': ers, 'tyre_size': tyre_size, 'handling': handling},
                        'source': rel, 'target': C.handling_archive(c)})
        root = vtf.parse(f0)
        tyre_group(root, c)
        rel = f'cars/handling/stock/{c.folder}_handling.erp'
        files[rel] = handling_archive(e, r, C.handling_names(c), vtf.write(root), vtf.tsmd(root, version))
        ops.append({'op': 'file.put', 'when': {'tyre_size': 'stock'}, 'source': rel, 'target': C.handling_archive(c)})
        info['car'] = f'{c.team}@{c.folder}'
        info['bytes'] = (len(f0), size)
        report.append(info)
    files['cars/handling.json'] = (json.dumps({'schema_version': 1, 'ops': ops}, indent=1) + '\n').encode()
    files['db/cars.json'] = (json.dumps(db_ops(), indent=1) + '\n').encode()
    files['lang/engines.json'] = (json.dumps(engine_lang(), indent=1) + '\n').encode()
    for rel, blob in files.items():
        dst = os.path.join(out, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        with open(dst, 'wb') as f:
            f.write(blob)
    return files, report


def chassis_key(c):
    return f'lng_f1990_chassis_{c.team}'


def engine_label(c):
    return rnd.engine_label(ENGINES[c.engine]['supplier'])


def engine_key(c):
    return f'lng_f1990_engine_{c.team}'


def package_ops(name, folder, **extra):
    values = dict(PACKAGE_SET, _metadata_=name,
                  resource_path=f'{folder}/teams/[FILENAME]/wep/[FILENAME]', **extra)
    return [{'op': 'add-row', 'table': 'entrants.vehicles.vehicle_packages',
             'from': f'entrants.vehicles.vehicle_packages.vehicle_package[_metadata_={C.SOURCE_PACKAGE_ROW}]',
             'set': values},
            {'op': 'set', 'select': f'entrants.vehicles.vehicle_packages.vehicle_package[_metadata_={name}]',
             'field': 'is_26_aero_regulations', 'value': 'False', 'when': {'classic_aero': True}},
            {'op': 'add-row', 'table': 'car_setup.presets',
             'from': f'car_setup.presets.vehicle_package_setups[_metadata_={C.SOURCE_PACKAGE}]',
             'set': {'_metadata_': folder}}]


def db_ops():
    with open(VEHICLE_FIELDS) as f:
        vehicle_fields = json.load(f)['fields']['entrants.vehicles.vehicle']
    vehicle_fields = vehicle_fields + [k for k in VEHICLE_EXTRA_FIELDS if k not in vehicle_fields]
    ops = package_ops(C.PACKAGE, C.PACKAGE_FOLDER)
    ops += package_ops(C.PACKAGE_B, C.PACKAGE_B_FOLDER, available='True')
    for row in engine_rows().values():
        ops.append({'op': 'add-row', 'table': ENGINE_TABLE,
                    'from': f'{ENGINE_TABLE}.Engine[_metadata_={ENGINE_TEMPLATE}]', 'set': row})
    for c in C.CARS:
        ops.append({'op': 'add-row', 'table': 'entrants.vehicles',
                    'from': f'entrants.vehicles.vehicle[_metadata_={c.donor_vehicle}]',
                    'set': {'_metadata_': c.vehicle, 'asset_filename': c.folder, 'engine': engine_label(c),
                            'vehicle_package': C.PACKAGE, 'hud_dial': '{{hud_dial}}', 'drs_capable': 'False',
                            'display_name': f'lng_f1990_team_{c.team}', 'chassis_name': chassis_key(c),
                            'engine_name_override': engine_key(c), 'equal_performance_vehicle': '',
                            'performance_tier': str(c.tier)}})
    for c in C.CARS:
        ops.append({'op': 'add-row', 'table': 'entrants.vehicles',
                    'from': f'entrants.vehicles.vehicle[_metadata_={c.vehicle}]',
                    'set': {'_metadata_': c.vehicle_b, 'vehicle_package': C.PACKAGE_B, 'engine': engine_label(c),
                            'available': 'True'}})
    return {'schema_version': 1,
            'fields': {'entrants.vehicles.vehicle_packages.vehicle_package': PACKAGE_FIELDS,
                       'entrants.vehicles.vehicle': vehicle_fields, f'{ENGINE_TABLE}.Engine': ENGINE_FIELDS},
            'ops': ops}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    ap.add_argument('--game', help='the F1 25 folder with stock 2026 team archives')
    ap.add_argument('--write', action='store_true', help='replace the shipped files with the build')
    args = ap.parse_args()
    out = ADDON if args.write else os.path.join(ADDON, 'build')
    files, report = build(args.game, out)
    for r in report:
        print(f'{r["car"]:28} {r["team"]:22} {r["engine"]:16} {r["hp"]:4} hp @ {r["peak_rpm"]}, limiter '
              f'{r["limiter_rpm"]} rpm  {r["mass"]:6} kg  MOI {r["moi"]}  COMZ {r["comz"]:+.4f}  '
              f'plank {r["plank"][0]:+.3f}/{r["plank"][1]:.3f}  drag x{r["drag"]}  final {r["final_drive"]}  {r["gears"]} gears')
    if args.write:
        return 0
    diff = [rel for rel, blob in files.items()
            if not os.path.exists(os.path.join(ADDON, rel)) or open(os.path.join(ADDON, rel), 'rb').read() != blob]
    for rel in diff:
        print(f'differs: {rel}')
    return 1 if diff else 0


if __name__ == '__main__':
    sys.exit(main())
