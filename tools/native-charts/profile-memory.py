"""Profile the actual Windows native chart window using supplied Chart JSON.

Input is a Chart array, or an array of Chart arrays to cycle pages every 3s.
Default gallery mode opens no game, user profile or addon helper. --section
profiles the full launcher and sample helper in an explicit separate profile.
The test window is visible.
"""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("executable", type=Path)
parser.add_argument("charts", type=Path)
parser.add_argument("--seconds", type=int, default=120)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--section", help="Use the full launcher and open addon-id/section-id")
parser.add_argument("--profile", type=Path, help="Separate prepared launcher profile for --section")
args = parser.parse_args()
if args.section and not args.profile:
    parser.error("--section requires an explicit separate --profile directory")


class Counters(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in (
            "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
            "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage",
            "PagefileUsage", "PeakPagefileUsage", "PrivateUsage",
        )
    ] + [("PrivateWorkingSetSize", ctypes.c_size_t), ("SharedCommitUsage", ctypes.c_uint64)]


get_memory = ctypes.WinDLL("psapi").GetProcessMemoryInfo
get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
get_memory.restype = wintypes.BOOL
exe = args.executable.resolve()
env = os.environ.copy()
env["SWOFTY_CHART_PROBE"] = str(args.charts.resolve())
command = [str(exe), "--chart-demo"]
if args.section:
    profile = args.profile.resolve()
    profile.mkdir(parents=True, exist_ok=True)
    config = profile / "config.json"
    if not config.exists():
        config.write_text(json.dumps({"onboarding": 1, "ui_mode": "pc", "home_video": False,
                                      "updates": {"check": False}}))
    env["SWOFTY_DATA_DIR"] = str(profile)
    env["SWOFTY_STEAM_ROOT_ONLY"] = str(profile / "empty-steam")
    env["SWOFTY_CHART_DEMO_VIEW"] = str(args.charts.resolve())
    env["SWOFTY_LAUNCHER_OPEN_PAGE"] = "section:" + args.section
    env["SWOFTY_CHART_METRICS"] = "1"
    command = [str(exe)]
# The portable package already contains DXC. No installation or download needed.
env["PATH"] = str(exe.parent.parent / "dxc" / "bin") + os.pathsep + env["PATH"]
rows = []
with args.output.with_suffix(".log").open("w") as log:
    process = subprocess.Popen(command, cwd=exe.parent,
                               env=env, stdout=log, stderr=log)
    try:
        start = time.monotonic()
        while time.monotonic() - start < args.seconds:
            if process.poll() is not None:
                raise RuntimeError(f"Chart window exited: {process.returncode}")
            counters = Counters()
            counters.cb = ctypes.sizeof(counters)
            if not get_memory(wintypes.HANDLE(int(process._handle)), ctypes.byref(counters),
                              ctypes.sizeof(counters)):
                raise ctypes.WinError()
            row = {"seconds": round(time.monotonic() - start, 1)}
            for name in ("WorkingSetSize", "PrivateWorkingSetSize", "PrivateUsage", "PeakWorkingSetSize", "PeakPagefileUsage"):
                row[name] = counters.__getattribute__(name)
            rows.append(row)
            if len(rows) % 10 == 0:
                print(json.dumps(row), flush=True)
            time.sleep(1)
    finally:
        process.terminate()
        process.wait(timeout=20)
        args.output.write_text(json.dumps(rows, indent=2))
