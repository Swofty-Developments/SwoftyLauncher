#!/usr/bin/env python3
"""Writes data/session-strings.json: the 1990 session names, set while a 1990 career is loaded.

usage: sessions.py <directory of the stock language_<code>.tsv tables>
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from career import DATA_GROUP, languages, read_tsv  # noqa: E402

OUT = os.path.join(os.path.dirname(HERE), "data", "session-strings.json")

FROM_STOCK = {
    "lng_sprintshootout_single": "lng_qualifying_1",
    "lng_sprintshootout_oneshot": "lng_qualifying_1",
    "lng_sprintshootout": "lng_qualifying_1",
    "lng_sprint_shootout": "lng_qualifying_1",
    "lng_sprintshootout_single_short": "lng_qualifying_1_short",
    "lng_sprintshootout_oneshot_short": "lng_qualifying_1_short",
    "lng_qualifying_length_short": "lng_qualifying_2",
    "lng_qualifying_length_one_shot": "lng_qualifying_2",
    "lng_qualifying": "lng_qualifying_2",
    "lng_qualifying_short_short": "lng_qualifying_2_short",
}
FIXED = {
    "lng_sprintshootout_results": "Qualifying 1 Results",
    "lng_qualifying_results": "Qualifying 2 Results",
}


def strings(lang_dir):
    out = {}
    for code in languages(lang_dir):
        table = read_tsv(os.path.join(lang_dir, f"language_{code}.tsv"))
        missing = sorted({source for source in FROM_STOCK.values() if source not in table})
        if missing:
            sys.exit(f"language_{code}.tsv has no {', '.join(missing)}")
        out[code] = {**{key: table[source] for key, source in FROM_STOCK.items()}, **FIXED}
    return {"scope": DATA_GROUP, "strings": out}


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    doc = strings(sys.argv[1])
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(doc, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(f"{OUT}: {len(doc['strings'])} languages, {sum(map(len, doc['strings'].values()))} strings")


if __name__ == "__main__":
    main()
