"""Checks a ninja deps log (<builddir>/.ninja_deps, format version 4) and removes it when a record is damaged.

Two ninja processes writing one build directory at once give the same node id to two path records. ninja 1.13 reports
such a log as "premature end of file; recovering" but leaves it as it is and appends after the damaged record, so
each later load drops every record from there on: every object's deps read as missing and every build recompiles
everything. Removing the log costs one full build, after which builds are incremental again.

Usage: ninja_deps.py LOG [WHO]. Prints one line when it removes LOG. A missing log, or one of another format version,
is left alone.
"""

import os
import struct
import sys

HEADER = b'# ninjadeps\n'
VERSION = 4
MAX_RECORD = (1 << 19) - 1


def damage(data):
    """Returns why the deps log data is damaged, or None when every record reads."""
    if not data.startswith(HEADER) or len(data) < len(HEADER) + 4:
        return 'no ninja deps header'
    if struct.unpack_from('<i', data, len(HEADER))[0] != VERSION:
        return None
    off = len(HEADER) + 4
    nodes = 0
    while off < len(data):
        if off + 4 > len(data):
            return f'a record size cut short at byte {off}'
        size = struct.unpack_from('<I', data, off)[0]
        is_deps = bool(size >> 31)
        size &= 0x7FFFFFFF
        if size > MAX_RECORD or off + 4 + size > len(data):
            return f'a record of {size} bytes at byte {off} runs past the log'
        body = data[off + 4 : off + 4 + size]
        if is_deps:
            if size < 12 or size % 4:
                return f'a deps record of {size} bytes at byte {off}'
            ids = struct.unpack_from(f'<{size // 4}i', body)
            if not all(0 <= i < nodes for i in (ids[0], *ids[3:])):
                return f'a deps record at byte {off} names a node the log has not defined'
        else:
            if size < 8 or size % 4:
                return f'a path record of {size} bytes at byte {off}'
            expected = ~struct.unpack_from('<I', body, size - 4)[0] & 0xFFFFFFFF
            if expected != nodes:
                path = body[: size - 4].rstrip(b'\0').decode('utf-8', 'replace')
                return f'the path record for {path} at byte {off} has node id {expected}, not {nodes}'
            nodes += 1
        off += 4 + size
    return None


def check(path, who='ninja_deps.py'):
    """Removes the deps log at path when it is damaged; returns the line it printed, or None."""
    try:
        with open(path, 'rb') as f:
            data = f.read()
    except FileNotFoundError:
        return None
    why = damage(data)
    if why is None:
        return None
    os.remove(path)
    line = f'{who}: removed {path} ({why}); this build recompiles everything once'
    print(line, flush=True)
    return line


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3):
        sys.exit('usage: ninja_deps.py LOG [WHO]')
    check(*sys.argv[1:])
