import sys
sys.path.insert(0, '/mnt/work/VSC/F1ModManager/tools')
import erpw

PK = 'F1 25/2025_asset_groups/f1_2026_vehicle_package/teams'
prefix = sys.argv[1]
bad = 0
for t in ['mclaren', 'ferrari', 'williams', 'lotus', 'mercedes', 'redbull', 'force_india', 'sauber', 'toro_rosso', 'gm_cadillac', 'haas']:
    a = erpw.Erp(f'/mnt/work/.mp45b/porter_a/stock26/{PK}/{t}/wep/{t}.erp')
    b = erpw.Erp(f'/mnt/work/.mp45b/porter_a/tries/{prefix}_{t}/{PK}/{t}/wep/{t}.erp')
    B = {r.name: r for r in b.res}
    vtf = all(all(f.raw == g.raw for f, g in zip(r.frags, B[r.name].frags)) and r.hash == B[r.name].hash
              for r in a.res if r.type.rstrip(b'\0') == b'VTF')
    same = set(r.name for r in a.res) == set(B)
    rep = open(f'/mnt/work/.mp45b/porter_a/tries/{prefix}_{t}/report.txt').read()
    log = open(f'/mnt/work/.mp45b/porter_a/tries/{prefix}_{t}/writer_log.txt').read()
    ok = 'SUCCESS' in rep and "'number' -> 'number'" in log
    print(t, 'patch+number', ok, 'VTF stock', vtf, 'resources same', same)
    bad += not (ok and vtf and same)
print('BAD', bad)
