#!/usr/bin/env python3
"""The archives logo_build_test writes, built without the slice cache, with a cold one and with a warm one, are the same
bytes as the ones listed in bc7/expect/logo_out.md5. That list names each archive by its folder and the MD5 of its
bytes; an archive's own name can carry a std::hash value, so it is left out. When the encoder's output changes on
purpose, bump SLICE_ENCODER in logo_build.hpp and write the list again with --write."""
import hashlib
import sys
from collections import Counter
from pathlib import Path


def digest(folder):
    out = Counter()
    for f in sorted(Path(folder).rglob("*")):
        if f.is_file():
            out[(f.parent.relative_to(folder).as_posix(), hashlib.md5(f.read_bytes()).hexdigest())] += 1
    return out


def text(c):
    return "".join(f"{md5}  {parent}\n" for (parent, md5), n in sorted(c.items()) for _ in range(n))


expect, folders = Path(sys.argv[1]), sys.argv[2:]
if folders and folders[0] == "--write":
    expect.write_text(text(digest(folders[1])))
    sys.exit(0)
want = expect.read_text()
fails = 0
for folder in folders:
    got = text(digest(folder))
    ok = got == want
    print(("ok    " if ok else "FAIL  ") + f"{folder}: {got.count(chr(10))} archive(s) match the expected bytes")
    fails += not ok
sys.exit(1 if fails else 0)
