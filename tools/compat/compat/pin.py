import functools
import subprocess
from pathlib import Path

PIN_SH = Path(__file__).resolve().parents[3] / "native" / "tools" / "pin.sh"


@functools.cache
def prefix():
    """The prefix native/tools/pin.sh gives heavy commands here: nothing under CI or without taskset, the
    maintainer's CPU mask with nice when this process may run on all of it and more, else nice alone."""
    try:
        out = subprocess.run(["bash", "-c", '. "$1"; printf "%s\\n" "${PIN[@]}"', "pin", str(PIN_SH)],
                             capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return []
    return out.split()
