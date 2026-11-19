#!/usr/bin/env python3
"""Moves the F1 25 modules of swofty-core under packages/core/src/games/f1_25/ as a pure move.

Re-runnable: every step checks what is already done, and a run on a moved tree changes nothing.
`--check` prints what a run would do and exits 1 when anything is left to do.
"""

import argparse
import os
import posixpath
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SRC = "packages/core/src"
INTO = f"{SRC}/games/f1_25"
F1_FILE = f"{SRC}/games/f1_25.rs"
F1_MOD = f"{INTO}/mod.rs"
LIB = f"{SRC}/lib.rs"
NEXTEST = ".config/nextest.toml"
BOUNDARY = "packages/core/boundary.json"
SKIP = ("tools/games-move/",)
TEXT_EXT = {".md", ".txt"}
COMMENT = ("//", "#", "*", "/*", "--", ";", "<!--", "|", "-")


def entries():
    lines = (HERE / "modules.txt").read_text().splitlines()
    return [line.strip() for line in lines if line.strip() and not line.startswith("#")]


ENTRIES = entries()
MODULES = [e.removesuffix(".rs") for e in ENTRIES]


def to_new(p):
    if p == F1_FILE:
        return F1_MOD
    for name in ENTRIES:
        old = f"{SRC}/{name}"
        if p == old or p.startswith(old + "/"):
            return f"{INTO}/{name}{p[len(old) :]}"
    return p


def to_old(p):
    if p == F1_MOD:
        return F1_FILE
    for name in ENTRIES:
        new = f"{INTO}/{name}"
        if p == new or p.startswith(new + "/"):
            return f"{SRC}/{name}{p[len(new) :]}"
    return p


def exists_after(p):
    if to_new(p) != p:
        return False
    return (ROOT / p).exists() or (ROOT / to_old(p)).exists()


def git(*args, check=True):
    out = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, check=False
    )
    if check and out.returncode != 0:
        sys.exit(f"git {' '.join(args)}: {out.stderr.strip()}")
    return out.stdout


def tracked():
    return [
        f for f in git("ls-files", "-z").split("\0") if f and not f.startswith(SKIP)
    ]


def read(rel):
    try:
        return (ROOT / rel).read_text(encoding="utf-8")
    except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
        return None


INCLUDE = re.compile(r'\b(include_str|include_bytes)!\(\s*"([^"]+)"\s*\)')
PATH_ATTR = re.compile(r'#\[path\s*=\s*"([^"]+)"\]')


def include_edits(files, problems):
    """The include_str!/include_bytes! paths that point elsewhere once the files move."""
    edits = {}
    for rel in files:
        if not rel.endswith(".rs"):
            continue
        text = read(rel)
        if text is None or "include_" not in text and "#[path" not in text:
            continue
        new_rel = to_new(rel)
        new_dir = posixpath.dirname(new_rel)
        old_dir = posixpath.dirname(to_old(new_rel))

        def fix(m, rel=rel, new_dir=new_dir, old_dir=old_dir):
            arg = m.group(2)
            if exists_after(posixpath.normpath(posixpath.join(new_dir, arg))):
                return m.group(0)
            target = to_new(posixpath.normpath(posixpath.join(old_dir, arg)))
            if not exists_after(target):
                problems.append(f'{rel}: {m.group(1)}!("{arg}") points at nothing')
                return m.group(0)
            return f'{m.group(1)}!("{posixpath.relpath(target, new_dir)}")'

        changed = INCLUDE.sub(fix, text)
        if to_new(rel) != rel and PATH_ATTR.search(text):
            problems.append(f"{rel}: has a #[path] attribute; move it by hand")
        if changed != text:
            edits[rel] = changed
    return edits


def mention_pattern():
    names = sorted((re.escape(e) for e in ENTRIES), key=len, reverse=True)
    return re.compile(rf"packages/core/src/({'|'.join(names)})\b")


def mention_edits(files, review):
    """Every packages/core/src/<module> path written in the repository, rewritten to its new place."""
    pattern = mention_pattern()
    edits = {}
    for rel in files:
        text = read(rel)
        if text is None or "packages/core/src/" not in text:
            continue
        out = []
        for n, line in enumerate(text.splitlines(keepends=True), 1):
            new = pattern.sub(lambda m: f"{INTO}/{m.group(1)}", line)
            if new != line:
                prose = Path(rel).suffix in TEXT_EXT or line.lstrip().startswith(
                    COMMENT
                )
                if not prose:
                    review.append(f"{rel}:{n}: {new.strip()}")
            out.append(new)
        changed = "".join(out)
        if changed != text:
            edits[rel] = edits.get(rel, changed)
    return edits


def visibility_edits(files, edits):
    """`pub(in crate::<module>)` names an ancestor, which a re-export is not; point it at the new place."""
    pattern = re.compile(
        rf"\bpub\(in crate::({'|'.join(re.escape(m) for m in MODULES)})\b"
    )
    found = {}
    for rel in files:
        if not rel.endswith(".rs"):
            continue
        text = edits.get(rel)
        if text is None:
            text = read(rel)
        if text is None or "pub(in crate::" not in text:
            continue
        changed = pattern.sub(
            lambda m: f"pub(in crate::games::f1_25::{m.group(1)}", text
        )
        if changed != text:
            found[rel] = changed
    return found


def nextest_edit(review):
    text = read(NEXTEST)
    if text is None:
        return {}
    names = "|".join(re.escape(m) for m in MODULES)
    changed = re.sub(
        rf"\btest\(([=~]?)(\^?)({names})::", r"test(\1\2games::f1_25::\3::", text
    )
    for n, line in enumerate(changed.splitlines(), 1):
        if re.search(rf"\btest\(/[^)]*\b({names})::", line):
            review.append(
                f"{NEXTEST}:{n}: a regex filter names a moved module: {line.strip()}"
            )
    return {NEXTEST: changed} if changed != text else {}


def module_moves():
    moves = []
    if (ROOT / F1_FILE).exists():
        if (ROOT / F1_MOD).exists():
            sys.exit(f"both {F1_FILE} and {F1_MOD} exist")
        moves.append((F1_FILE, F1_MOD))
    for name in ENTRIES:
        old, new = f"{SRC}/{name}", f"{INTO}/{name}"
        if (ROOT / old).exists():
            if (ROOT / new).exists():
                sys.exit(f"both {old} and {new} exist")
            moves.append((old, new))
        elif not (ROOT / new).exists():
            sys.exit(f"{name} is in neither {SRC} nor {INTO}")
    return moves


REEXPORT = re.compile(r"pub use games::f1_25::\{([^}]*)\};\n")


def lib_edit(text):
    kept = [
        line
        for line in text.splitlines(keepends=True)
        if not any(line == f"pub mod {m};\n" for m in MODULES)
    ]
    text = "".join(kept)
    names = sorted(MODULES)
    line = f"pub use games::f1_25::{{{', '.join(names)}}};\n"
    found = REEXPORT.search(text)
    if found:
        have = sorted(n.strip() for n in found.group(1).split(",") if n.strip())
        return text if have == names else text.replace(found.group(0), line)
    mods = [m.end() for m in re.finditer(r"^pub mod \w+;\n", text, re.MULTILINE)]
    at = mods[-1] if mods else 0
    return text[:at] + "\n" + line + text[at:]


def f1_mod_edit(text):
    missing = [
        m for m in MODULES if not re.search(rf"^pub mod {m};$", text, re.MULTILINE)
    ]
    if not missing:
        return text
    lines = text.splitlines(keepends=True)
    at = 0
    while at < len(lines) and lines[at].startswith("//!"):
        at += 1
    while at < len(lines) and not lines[at].strip():
        at += 1
    block = "".join(f"pub mod {m};\n" for m in missing) + "\n"
    head = "".join(lines[:at])
    if head and not head.endswith("\n\n"):
        head += "\n"
    return head + block + "".join(lines[at:])


def plan():
    files = tracked()
    problems, review = [], []
    edits = include_edits(files, problems)
    for rel, text in mention_edits(files, review).items():
        if rel in edits:
            base = edits[rel]
            edits[rel] = mention_pattern().sub(lambda m: f"{INTO}/{m.group(1)}", base)
        else:
            edits[rel] = text
    edits.update(visibility_edits(files, edits))
    edits.update(nextest_edit(review))
    return edits, module_moves(), problems, review


def apply_after_moves(check):
    changes = []
    lib = read(LIB)
    new_lib = lib_edit(lib)
    if new_lib != lib:
        changes.append(LIB)
        if not check:
            (ROOT / LIB).write_text(new_lib)
    mod_path = F1_MOD if (ROOT / F1_MOD).exists() else F1_FILE
    mod = read(mod_path)
    new_mod = f1_mod_edit(mod)
    if new_mod != mod:
        changes.append(F1_MOD)
        if not check:
            (ROOT / mod_path).write_text(new_mod)
    return changes


def cargo(*args, env=None):
    print("+ cargo", " ".join(args), flush=True)
    full = dict(os.environ, **(env or {}))
    if (
        subprocess.run(["cargo", *args], cwd=ROOT, env=full, check=False).returncode
        != 0
    ):
        sys.exit(f"cargo {' '.join(args)} failed")


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--check",
        action="store_true",
        help="print what a run would do; exit 1 if anything is left",
    )
    ap.add_argument(
        "--expect", metavar="SHA", help="refuse to run unless HEAD is this commit"
    )
    ap.add_argument(
        "--no-cargo",
        action="store_true",
        help="skip cargo fmt and writing boundary.json",
    )
    ap.add_argument(
        "--verify",
        action="store_true",
        help="after the move, run check, clippy and the tests",
    )
    args = ap.parse_args()

    if args.expect:
        head = git("rev-parse", "HEAD").strip()
        want = git("rev-parse", "--verify", f"{args.expect}^{{commit}}").strip()
        if head != want:
            sys.exit(f"HEAD is {head[:12]}, not the agreed tip {want[:12]}")
    if not args.check and git("status", "--porcelain"):
        sys.exit("the working tree has changes; commit or remove them first")

    edits, moves, problems, review = plan()
    if problems:
        print("cannot move:", *problems, sep="\n  ")
        sys.exit(2)

    for rel in sorted(edits):
        print(f"edit {rel}")
    for old, new in moves:
        print(f"move {old} -> {new}")
    if args.check:
        later = apply_after_moves(check=True) if not moves else [LIB, F1_MOD]
        for rel in later:
            print(f"edit {rel}")
        if not (ROOT / BOUNDARY).exists():
            print(f"write {BOUNDARY}")
        if review:
            print("check by hand (code, not prose):", *review, sep="\n  ")
        pending = edits or moves or later or not (ROOT / BOUNDARY).exists()
        print("nothing to do" if not pending else "changes pending")
        sys.exit(1 if pending else 0)

    for rel, text in edits.items():
        (ROOT / rel).write_text(text)
    if moves:
        (ROOT / INTO).mkdir(parents=True, exist_ok=True)
    for old, new in moves:
        git("mv", old, new)
    for rel in apply_after_moves(check=False):
        print(f"edit {rel}")
    git("add", "-A")

    if not args.no_cargo:
        cargo("fmt", "--all")
        if not (ROOT / BOUNDARY).exists():
            cargo(
                "test",
                "-p",
                "swofty-core",
                "--test",
                "boundary",
                env={"SWOFTY_BOUNDARY_UPDATE": "1"},
            )
        git("add", "-A")
    if review:
        print("rewritten, check by hand (code, not prose):", *review, sep="\n  ")
    if args.verify:
        cargo("check", "--workspace", "--all-targets")
        cargo("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
        cargo("test", "--workspace")
    print("done; review `git diff --cached -M --stat` and commit")


if __name__ == "__main__":
    main()
