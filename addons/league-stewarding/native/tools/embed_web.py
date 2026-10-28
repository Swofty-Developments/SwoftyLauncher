#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
import os, sys

MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".json": "application/json",
}

STUB = b"<!doctype html><meta charset=utf-8><title>League Stewarding</title><p>The panel files were not built into this copy of the addon."


def main():
    web, out = sys.argv[1], sys.argv[2]
    files = []
    if os.path.isdir(web):
        for name in sorted(os.listdir(web)):
            path = os.path.join(web, name)
            if os.path.isfile(path):
                files.append((name, open(path, "rb").read()))
    if not any(n == "index.html" for n, _ in files):
        files.append(("index.html", STUB))
    lines = ["#pragma once", '#include "router.hpp"', "namespace steward {"]
    for i, (name, data) in enumerate(files):
        body = ",".join(str(b) for b in data) or "0"
        lines.append(f"inline const char kWeb{i}[] = {{{body}}};")
    lines.append("inline const Asset kWebAssets[] = {")
    for i, (name, data) in enumerate(files):
        mime = MIME.get(os.path.splitext(name)[1].lower(), "application/octet-stream")
        lines.append(f'    {{"{name}", "{mime}", std::string_view(kWeb{i}, {len(data)})}},')
    lines.append("};")
    lines.append("}")
    tmp = out + ".tmp"
    open(tmp, "w").write("\n".join(lines) + "\n")
    os.replace(tmp, out)


main()
