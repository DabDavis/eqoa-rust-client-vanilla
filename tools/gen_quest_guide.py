#!/usr/bin/env python3
"""The quest guide: writes crates/eqoa/src/questguide/data.rs.

What the client's own quest marks and tracker estimate from (`questguide`),
on a server that sends none of its own (Sandstorm): EQOA-Data's 1-20 city
class quests (github.com/Jadiction/EQOA-Data, MIT: Quests/1-20/<city>/<class>
.json), as go-eqoa-server's tools/quests/data/<city>.json hold them (746
quests in 13 cities, each a chain of a class), with only what the marks and
the tracker need:

  - each quest's key, name, city, classes, races, levels, the quests it
    requires (indices), its giver, the first line of the giver's offer (as
    the server's own marks clip it: 140 bytes) and its rewards in all
    ("2200 XP, Kick", as the server words them);
  - each step's kind, its NPC or creature, the items it needs and how many,
    the items it hands over when done (its `give` events), a kill step's
    count, a give step's tunar, a reach step's place, and the guide's
    journal line;
  - each NPC's and creature's spawns (world, x, z), from a copy of
    go-eqoa-server's eqoa.db (`npcs`), best seeded first with the quests'
    missing people (its tools/seed_quest_npcs.py), every name the files use
    then having one;
  - each race and class's home cities: the city whose chain for the class
    starts nearest the start spots of the database's `defaultcharacter`
    (one a race, two for a human's east and west).

    python3 -E -s tools/gen_quest_guide.py --server ../go-eqoa-server \\
        --db <a seeded copy of go-eqoa-server's eqoa.db> \\
        [--out crates/eqoa/src/questguide/data.rs]

The database is read only. Without --out it prints what it would write.
"""

import argparse
import collections
import glob
import json
import math
import os
import sqlite3
import sys

KINDS = {"talk": "Talk", "show": "Show", "give": "Give", "kill": "Kill", "loot": "Loot", "reach": "Reach"}
# The server's own marks clip a text to this many bytes (questmarks.go's maxMarkText).
MAX_TEXT = 140
# Words a title-cased item name keeps small (not its first).
SMALL = {"a", "an", "and", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "with"}


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def fold(name):
    """A name as the server folds one (foldName): trimmed, lower case."""
    return " ".join(name.split()).lower()


def clip(s, n=MAX_TEXT):
    """s within n bytes, cut at a character and marked (questmarks.go's clip)."""
    s = s.strip()
    b = s.encode()
    if len(b) <= n:
        return s
    cut = n - 3
    while cut > 0 and (b[cut] & 0xC0) == 0x80:
        cut -= 1
    return b[:cut].decode().strip() + "..."


def title(name):
    """An item as a player reads it: each word capitalised but the small ones."""
    words = name.split()
    out = []
    for i, w in enumerate(words):
        if i > 0 and w.lower() in SMALL:
            out.append(w.lower())
        else:
            out.append(w[:1].upper() + w[1:])
    return " ".join(out)


def count_of(n, name):
    """quests.go's countOf: "2 Bat Wing", or the name alone for one."""
    return name if n == 1 else f"{n} {name}"


def offer_line(offer):
    """The offer's first line, the giver's own words (questTexts): a leading
    "Name: " under 40 characters taken off."""
    for line in offer.replace("\r", "").split("\n"):
        line = line.strip()
        if line:
            i = line.find(": ")
            if 0 < i < 40:
                line = line[i + 2:]
            return clip(line)
    return ""


def rewards_text(steps):
    """The quest's rewards in all, as questTexts words them: XP, tunar, items, spells."""
    xp = tunar = 0
    got = []
    for st in steps:
        r = st.get("rewards") or {}
        xp += int(r.get("xp", 0) or 0)
        tunar += int(r.get("tunar", 0) or 0)
        for it in r.get("items", []) or []:
            if isinstance(it, dict):
                got.append(count_of(max(int(it.get("count", 1) or 1), 1), title(it["item"])))
            else:
                got.append(title(it))
        got.extend(r.get("spells", []) or [])
    out = []
    if xp:
        out.append(f"{xp} XP")
    if tunar:
        out.append(f"{tunar} tunar")
    return clip(", ".join(out + got))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--server", default="../go-eqoa-server")
    ap.add_argument("--db", required=True)
    ap.add_argument("--out")
    a = ap.parse_args()

    files = sorted(glob.glob(os.path.join(a.server, "tools", "quests", "data", "*.json")))
    if not files:
        sys.exit(f"no quest files under {a.server}/tools/quests/data")
    cities, quests = [], []
    for f in files:
        d = json.load(open(f, encoding="utf-8"))
        cities.append(d["city"])
        for q in d["quests"]:
            quests.append((len(cities) - 1, q))
    index = {q["key"]: i for i, (_, q) in enumerate(quests)}
    if len(index) != len(quests):
        sys.exit("two quests share a key")

    db = sqlite3.connect(f"file:{a.db}?mode=ro", uri=True)
    spawns = collections.defaultdict(set)
    for name, world, x, z in db.execute("SELECT npc_name, world, x_coord, z_coord FROM npcs"):
        if name and world is not None and x is not None and z is not None:
            spawns[fold(name)].add((int(world), round(float(x), 1), round(float(z), 1)))

    # every NPC and creature the quests name, in the order first named, as
    # the files spell it
    npcs, npc_index = [], {}

    def npc(name):
        k = fold(name)
        if k not in npc_index:
            npc_index[k] = len(npcs)
            npcs.append((name.strip(), sorted(spawns.get(k, ()))))
        return npc_index[k]

    out_quests = []
    for city, q in quests:
        steps = []
        for st in q["steps"]:
            kind = KINDS[st["kind"]]
            target = npc(st["target"]) if st.get("target") else None
            items = [(title(it["item"]), max(int(it.get("count", 1) or 1), 1)) for it in st.get("items", []) or []]
            if kind == "Loot" and items:
                # the drops to take: the step's count, else its item's
                items = [(items[0][0], max(int(st.get("count", 0) or 0), items[0][1]))]
            gets = []
            for e in st.get("events", []) or []:
                if e.get("on") == "done" and e.get("do") == "give":
                    gets.extend(title(it["item"]) for it in e.get("items", []) or [])
            at = None
            if kind == "Reach" and st.get("world") is not None:
                at = (int(st["world"]), round(float(st["x"]), 1), round(float(st["z"]), 1))
            steps.append({
                "kind": kind,
                "target": target,
                "items": items,
                "gets": gets,
                "count": int(st.get("count", 0) or 0) if kind == "Kill" else 0,
                "tunar": int(st.get("tunar", 0) or 0) if kind == "Give" else 0,
                "at": at,
                "journal": clip(st.get("journal", "")),
            })
        classes = 0
        for c in q.get("classes", []):
            classes |= 1 << int(c)
        races = 0
        for r in q.get("races", []):
            races |= 1 << int(r)
        out_quests.append({
            "key": q["key"],
            "name": q["name"],
            "city": city,
            "classes": classes,
            "races": races,
            "levels": (int(q.get("minlevel", 1)), int(q.get("maxlevel", 60))),
            "requires": [index[k] for k in q.get("requires", []) if k in index],
            "giver": npc(q["giver"]),
            "offer": offer_line(q.get("offer", "")),
            "rewards": rewards_text(q["steps"]),
            "steps": steps,
        })
    unplaced = [n for n, p in npcs if not p]
    print(f"{len(out_quests)} quests, {len(npcs)} NPCs and creatures with {sum(len(p) for _, p in npcs)} spawns; "
          f"{len(unplaced)} with none: {unplaced[:12]}", file=sys.stderr)

    # each race and class's home cities: the city whose chain for the class
    # begins nearest each start spot
    heads = collections.defaultdict(list)  # class -> [(city, world, x, z)]
    for q in out_quests:
        if q["requires"]:
            continue
        for c in range(15):
            if q["classes"] & (1 << c):
                for w, x, z in npcs[q["giver"]][1]:
                    heads[c].append((q["city"], w, x, z))
    homes = collections.defaultdict(set)
    for race, klass, world, x, z in db.execute("SELECT race, tclass, world, x, z FROM defaultcharacter"):
        best = None
        for city, w, hx, hz in heads.get(int(klass), []):
            if w != int(world or 0):
                continue
            d = math.hypot(hx - x, hz - z)
            if best is None or d < best[0]:
                best = (d, city)
        if best is not None and best[0] < 1500:
            homes[(int(race), int(klass))].add(best[1])
    print(f"{len(homes)} race and class pairs with a home city", file=sys.stderr)

    text = render(cities, npcs, out_quests, homes)
    if a.out:
        open(a.out, "w", encoding="utf-8").write(text)
        print(f"{a.out}: {len(text.encode())} bytes", file=sys.stderr)
    else:
        print(text)


def render(cities, npcs, quests, homes):
    o = []
    o.append("// Code written by tools/gen_quest_guide.py; DO NOT EDIT: change the tool and run it again.")
    o.append("//")
    o.append("// EQOA-Data's 1-20 city class quests (github.com/Jadiction/EQOA-Data, MIT), as go-eqoa-server's")
    o.append("// tools/quests/data holds them, with only what the client's own quest marks and tracker need;")
    o.append("// the NPCs' and creatures' places are go-eqoa-server's spawns (its npcs, the quests' missing")
    o.append("// people placed by its tools/seed_quest_npcs.py), and each race and class's home cities the")
    o.append("// city whose chain for the class begins nearest its start spots there (defaultcharacter).")
    o.append("")
    o.append("use super::Kind::{Give, Kill, Loot, Reach, Show, Talk};")
    o.append("use super::{Home, NONE, Npc, Quest, st};")
    o.append("")
    o.append("/// The cities, as the quest files name them.")
    o.append(f"pub const CITIES: [&str; {len(cities)}] = [{', '.join(rust_str(c) for c in cities)}];")
    o.append("")
    o.append("/// Every NPC and creature the quests name, each with its spawns: world, x, z.")
    o.append(f"pub const NPCS: [Npc; {len(npcs)}] = [")
    for name, places in npcs:
        p = ", ".join(f"({w}, {x:.1f}, {z:.1f})" for w, x, z in places)
        o.append(f"    Npc {{ name: {rust_str(name)}, places: &[{p}] }},")
    o.append("];")
    o.append("")
    o.append("/// The quests, each step `st(kind, NPC or creature, items needed, items handed over, kills, tunar,")
    o.append("/// a reach step's place, the journal line)`.")
    o.append(f"pub const QUESTS: [Quest; {len(quests)}] = [")
    for q in quests:
        steps = []
        for s in q["steps"]:
            target = "NONE" if s["target"] is None else str(s["target"])
            items = ", ".join(f"({rust_str(n)}, {c})" for n, c in s["items"])
            gets = ", ".join(rust_str(n) for n in s["gets"])
            at = "None" if s["at"] is None else f"Some(({s['at'][0]}, {s['at'][1]:.1f}, {s['at'][2]:.1f}))"
            steps.append(f"st({s['kind']}, {target}, &[{items}], &[{gets}], {s['count']}, {s['tunar']}, {at}, {rust_str(s['journal'])})")
        req = ", ".join(str(r) for r in q["requires"])
        o.append(
            f"    Quest {{ key: {rust_str(q['key'])}, name: {rust_str(q['name'])}, city: {q['city']}, "
            f"classes: {q['classes']:#06x}, races: {q['races']:#06x}, levels: ({q['levels'][0]}, {q['levels'][1]}), "
            f"requires: &[{req}], giver: {q['giver']}, offer: {rust_str(q['offer'])}, rewards: {rust_str(q['rewards'])}, "
            f"steps: &[{', '.join(steps)}] }},"
        )
    o.append("];")
    o.append("")
    o.append("/// Each race and class's home cities (`CITIES`), by where the game starts it.")
    o.append(f"pub const HOMES: [Home; {len(homes)}] = [")
    for (race, klass), cs in sorted(homes.items()):
        o.append(f"    Home {{ race: {race}, class: {klass}, cities: &[{', '.join(str(c) for c in sorted(cs))}] }},")
    o.append("];")
    return "\n".join(o) + "\n"


if __name__ == "__main__":
    main()
