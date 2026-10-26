#!/usr/bin/env python3
"""The map's markers: writes crates/eqoa/src/map/markers_data.rs.

Three kinds, each drawn on the map (M) at its place, the first two with a
tooltip:

  - EQOA-Data's XP camps (github.com/Jadiction/EQOA-Data, MIT: Map/zones.json's
    pois of category "camp", the 69 with a note being its XP-camp guide), the
    49 go-eqoa-server places (its internal/handlers/camps.go, written by its
    tools/gen_camp_teles.py: each camp on one of its creatures' spawns, where
    the guide's map point was off on the creatures' cluster): the guide's name,
    level range and note, the rare and raid creatures EQOA-Data's map marks
    near it, and the notable items of its zone's biome loot table at its
    levels (EQOA-Data's items name their drop as one of thirteen biome tables;
    its zones list their quadrants' biomes), as go-eqoa-server's
    tools/seed_camp_loot.py binds the camp's creatures to it.
  - The coaches: each coachman at its spawn, the town go-eqoa-server's
    internal/entity/coachtowns.go names for it (its coachLocationNames) and the
    towns its coach runs to (EQOA-Data's Map/coach_map.json, in those names; a
    town only named as a destination runs to the towns naming it); and the
    routes as lines, each pair of towns the coach map joins once, both ends in
    one world (a coach's place, else the town's: Kerplunk Outpost has no
    coachman; Oggok's routes, out of Rathe into Tunaria, left out).
  - The towns: EQOA-Data's Map/zones.json towns, each in its world: Tunaria's
    where coachtowns.go's coachTowns puts it (the same points by its fit, in
    the names its coach menus use), Odus's by the camps' fit below, Oggok in
    Rathe (Frontiers' Tunaria has no zone where the sheet puts it, past its
    Oggok Gate; Coachman Dilga stands in Rathe's Oggok); two of a name told
    apart by their sides, as coachTowns tells Hazinak's. Each with its side
    and its size, the spawns within TOWN_REACH of it, the order its name gives
    way in when names crowd.

Where the rare creatures stand: EQOA-Data's map points laid over the game by a
fit of the camps' own points to where the camps are (Tunaria and Odus each
their own, printed with how close they come).

    python3 -E -s tools/gen_map_markers.py --server ../go-eqoa-server \\
        --eqoa-data ../external/EQOA-Data --db <a copy of go-eqoa-server's eqoa.db> \\
        [--out crates/eqoa/src/map/markers_data.rs]

The database is read only, for the coachmen's spawns (npcs). Without --out it
prints what it would write.
"""

import argparse
import collections
import importlib.util
import json
import math
import os
import re
import sqlite3
import sys

# How far from a camp a rare creature counts as the camp's, in game units, by
# world: Tunaria's fit comes within a few units, Odus's within a few hundred.
RARE_REACH = {0: 700.0, 2: 1500.0}
# The notable drops listed: the items of the camp's table at its levels
# (within this many of its range), the highest first.
DROPS = 4
DROP_SLACK = 2
# Where Odus is on EQOA-Data's sheet: the southwest (its camps' points all lie
# there, Tunaria's none).
def map_world(mx, my):
    return 2 if mx < 3700 and my > 7400 else 0


# Coachmen coachtowns.go names no town for: Coachman Dilga stands in Oggok (the
# map's own area name there), Rathe's only coach, which go-eqoa-server's coach
# menus leave out as no route of theirs leaves Rathe; EQOA-Data's coach map has
# Oggok's.
EXTRA_TOWNS = {"Coachman Dilga": "Oggok"}

# Towns not where EQOA-Data's sheet puts them: at this coachman's spawn.
TOWN_AT_COACH = {"Oggok": "Coachman Dilga"}
# A town's size: the spawns within this many units of its place.
TOWN_REACH = 600.0

# EQOA-Data's coach towns by the name coachtowns.go gives them (the names the
# coach menus list), where the two differ.
COACH_NAMES = {
    "Neriak": "Neriak, Nektulos",
    "Fort Seriak": "Castle Felstar",
    "Klik'Anon": "Klick'Anon",
    "Hazinak (Good)": "Hazinak-Good",
    "Hazinak (Evil)": "Hazinak-Evil",
    "Stone of Morathals (Good)": "Stone of Morathals-Good",
    "Stone of Morathals (Evil)": "Stone of Morathals-Evil",
}


def module(path, name):
    sys.path.insert(0, os.path.dirname(path))
    spec = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def server_camps(path):
    """camps.go's campDests: key -> (x, z, world, label)."""
    text = open(path).read()
    out = collections.OrderedDict()
    for m in re.finditer(r'"(\w+)":\s*\{NPC: "[^"]*", Near: true, X: ([-\d.]+), Y: [-\d.]+, Z: ([-\d.]+), World: (\d+), Levels: "[^"]*", Label: "([^"]*)"\}', text):
        out[m.group(1)] = (float(m.group(2)), float(m.group(3)), int(m.group(4)), m.group(5))
    return out


def coach_towns(path):
    """coachtowns.go's coachLocationNames: coachman -> town."""
    text = open(path).read()
    block = text[text.index("var coachLocationNames"):]
    block = block[: block.index("\n}\n")]
    return dict(re.findall(r'"([^"]+)":\s*"([^"]+)"', block))


def coach_town_places(path):
    """coachtowns.go's coachTowns: town -> (x, z), Tunaria's."""
    text = open(path).read()
    block = text[text.index("var coachTowns"):]
    block = block[: block.index("\n}\n")]
    return {n: (float(x), float(z)) for n, x, z in re.findall(r'\{"([^"]+)",\s*([-\d.]+),\s*([-\d.]+)\}', block)}


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def clean_poi(name):
    """A rare creature's name off the map: the parenthesised aside gone."""
    return re.sub(r"\s*\([^)]*\)\s*", " ", name).strip()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--server", default="../go-eqoa-server")
    ap.add_argument("--eqoa-data", default="../external/EQOA-Data")
    ap.add_argument("--db", required=True)
    ap.add_argument("--out")
    a = ap.parse_args()

    teles = module(os.path.join(a.server, "tools", "gen_camp_teles.py"), "gen_camp_teles")
    loot = module(os.path.join(a.server, "tools", "seed_camp_loot.py"), "seed_camp_loot")
    zones = json.load(open(os.path.join(a.eqoa_data, "Map", "zones.json")))
    placed = server_camps(os.path.join(a.server, "internal", "handlers", "camps.go"))
    specs = {s[0]: s for s in teles.CAMPS}

    # each camp: the guide's points, name, levels and note
    camps = []
    for key, (gx, gz, world, label) in placed.items():
        spec = specs[key]
        pts = [q for q in zones["pois"] if q["category"] == "camp" and q["name"] == spec[2] and q.get("note")]
        if spec[3] is not None:
            pts = [pts[i] for i in spec[3] if i < len(pts)]
        if not pts:
            sys.exit(f"{key}: no point of {spec[2]!r} on EQOA-Data's map")
        mx = sum(q["x"] for q in pts) / len(pts)
        my = sum(q["y"] for q in pts) / len(pts)
        ranges = sorted({q.get("levelRange") for q in pts if q.get("levelRange")})
        lo = min(int(r.split("-")[0]) for r in ranges)
        hi = max(int(r.split("-")[-1]) for r in ranges)
        note = pts[0]["note"]
        # the guide's own reminders to itself
        note = re.sub(r"\s*Needs review:.*$", "", note).strip()
        camps.append(dict(key=key, name=spec[2], label=label, world=world, at=(gx, gz), map=(mx, my), lo=lo, hi=hi, note=note))

    # the map laid over the game by the camps themselves
    fits = {}
    for w in (0, 2):
        pairs = [(c["map"][0], c["map"][1], c["at"][0], c["at"][1]) for c in camps if c["world"] == w]
        (fa, fb, fc, fd), kept, n, med = teles.fit(pairs)
        fits[w] = (fa, fb, fc, fd)
        print(f"world {w}: game x = {fa:.5f} map x + {fb:.1f}, z = {fc:.5f} map y + {fd:.1f} ({kept} of {n} camps, median {med:.0f} off)", file=sys.stderr)

    rares = []
    for q in zones["pois"]:
        if q["category"] not in ("rare-mob", "raid-mob"):
            continue
        w = map_world(q["x"], q["y"])
        fa, fb, fc, fd = fits[w]
        rares.append((clean_poi(q["name"]), q["category"], w, (fa * q["x"] + fb, fc * q["y"] + fd)))

    tables = loot.items_by_table(a.eqoa_data)
    for c in camps:
        near = sorted(
            (math.hypot(r[3][0] - c["at"][0], r[3][1] - c["at"][1]), r[0], r[1])
            for r in rares
            if r[2] == c["world"] and math.hypot(r[3][0] - c["at"][0], r[3][1] - c["at"][1]) <= RARE_REACH[c["world"]]
        )
        c["rare"] = [n + (" (raid)" if kind == "raid-mob" else "") for _, n, kind in near]
        # a rare creature's own drops, where EQOA-Data names it as their source
        own = []
        for _, n, _ in near:
            for name, _lv in tables.get(loot.table_of(n), []):
                if name not in own:
                    own.append(name)
        c["own"] = own
        biome = loot.biome_at(zones, *c["map"])[0] if c["world"] == 0 else None
        c["table"] = biome or ""
        drops = []
        if biome:
            seen = set()
            items = [(lv, name) for name, lv in tables.get(biome.lower(), []) if lv is not None and c["lo"] - DROP_SLACK <= lv <= c["hi"] + DROP_SLACK]
            for lv, name in sorted(items, key=lambda t: (-t[0], t[1].lower())):
                if name.lower() in seen:
                    continue
                seen.add(name.lower())
                drops.append(name)
                if len(drops) == DROPS:
                    break
        c["drops"] = drops
        print(f"{c['key']:15} {c['name'][:28]:28} L{c['lo']}-{c['hi']:<3} {c['table'] or '-':12} rare {c['rare']} drops {drops}", file=sys.stderr)

    # the coaches: each coachman at its spawn, its town and where it runs
    db = sqlite3.connect(f"file:{a.db}?mode=ro", uri=True)
    spawns = {r[0].strip(): r[1:] for r in db.execute("SELECT npc_name, world, x_coord, z_coord FROM npcs WHERE npc_name LIKE 'Coach%' ORDER BY id")}
    served = coach_towns(os.path.join(a.server, "internal", "entity", "coachtowns.go"))
    net = {k.strip(): [d.strip() for d in v] for k, v in json.load(open(os.path.join(a.eqoa_data, "Map", "coach_map.json"))).items()}
    ours = {v: k for k, v in COACH_NAMES.items()}

    def runs(town):
        key = COACH_NAMES.get(town, town)
        to = net.get(key)
        if to is None:
            to = [t for t, ds in net.items() if key in ds]
        return [ours.get(t, t) for t in to]

    coaches = []
    for man in sorted(spawns):
        world, x, z = spawns[man]
        town = served.get(man) or EXTRA_TOWNS.get(man, "")
        to = runs(town) if town else []
        coaches.append(dict(man=man, town=town, world=int(world), at=(float(x), float(z)), to=to))
        print(f"coach {man:22} {town or '-':20} w{world} ({x:.0f}, {z:.0f}) -> {to}", file=sys.stderr)
    missing = [t for t in served.values() if not runs(t)]
    if missing:
        print(f"towns with no route on EQOA-Data's map: {missing}", file=sys.stderr)

    # the towns, each in its world, the biggest first
    places = coach_town_places(os.path.join(a.server, "internal", "entity", "coachtowns.go"))
    count = collections.Counter(q["name"] for q in zones["towns"])
    spots = [(int(r[0]), float(r[1]), float(r[2])) for r in db.execute("SELECT world, x_coord, z_coord FROM npcs")]
    towns = []
    for q in zones["towns"]:
        side = q.get("alignment", "neutral")
        name = f"{q['name']} ({side.title()})" if count[q["name"]] > 1 else q["name"]
        name = ours.get(name, name)
        w = map_world(q["x"], q["y"])
        if name in TOWN_AT_COACH:
            world, x, z = spawns[TOWN_AT_COACH[name]]
            w, at_ = int(world), (float(x), float(z))
        elif w == 0 and name in places:
            at_ = places[name]
        else:
            if w == 0:
                print(f"town {name}: not in coachTowns, placed by the fit", file=sys.stderr)
            fa, fb, fc, fd = fits[w]
            at_ = (fa * q["x"] + fb, fc * q["y"] + fd)
        size = sum(1 for sw, sx, sz in spots if sw == w and math.hypot(sx - at_[0], sz - at_[1]) <= TOWN_REACH)
        towns.append(dict(name=name, world=w, at=at_, side=side, size=size))
    towns.sort(key=lambda t: (-t["size"], t["name"]))
    for t in towns:
        print(f"town {t['name']:26} w{t['world']} ({t['at'][0]:.0f}, {t['at'][1]:.0f}) {t['side']:8} {t['size']} spawns", file=sys.stderr)

    # the routes: each pair the coach map joins once, both ends a stop (a
    # coach, else the town) of one world
    stops = {t["name"]: (t["world"], t["at"]) for t in towns}
    stops.update({c["town"]: (c["world"], c["at"]) for c in coaches if c["town"]})
    routes, left = {}, set()
    for k, ds in net.items():
        for d in ds:
            pair = tuple(sorted((ours.get(k, k), ours.get(d, d))))
            ends = [stops.get(n) for n in pair]
            if None in ends or ends[0][0] != ends[1][0]:
                left.add(pair)
                continue
            routes[pair] = (ends[0][0], ends[0][1], ends[1][1])
    routes = sorted((p, *v) for p, v in routes.items())
    print(f"{len(routes)} routes; left out (no place, or another world): {sorted(left)}", file=sys.stderr)

    out = render(camps, coaches, towns, routes)
    if a.out:
        open(a.out, "w").write(out)
        print(f"{a.out}: {len(camps)} camps, {len(coaches)} coaches, {len(towns)} towns, {len(routes)} routes", file=sys.stderr)
    else:
        print(out)


def strs(v):
    return "&[" + ", ".join(rust_str(s) for s in v) + "]"


def render(camps, coaches, towns, routes):
    o = []
    o.append("// Code written by tools/gen_map_markers.py; DO NOT EDIT: change the tool and run it again.")
    o.append("//")
    o.append("// The XP camps are EQOA-Data's (github.com/Jadiction/EQOA-Data, MIT: its map's camps with a")
    o.append("// note, its XP-camp guide; their rare creatures and its biome loot tables' items), each at the")
    o.append("// place go-eqoa-server's tools/gen_camp_teles.py gives it; the coaches are each coachman's")
    o.append("// spawn, the town go-eqoa-server's coach menus name for it and the towns EQOA-Data's coach map")
    o.append("// has its coach run to; the towns are EQOA-Data's, Tunaria's where go-eqoa-server's coach menus")
    o.append("// put them, and the routes its coach map's.")
    o.append("")
    o.append("use super::markers::{Camp, Coach, Route, Side, Town};")
    o.append("")
    o.append(f"pub const CAMPS: [Camp; {len(camps)}] = [")
    for c in camps:
        o.append(
            "    Camp { "
            f"name: {rust_str(c['name'])}, place: {rust_str(c['label'])}, world: {c['world']}, at: [{c['at'][0]:.1f}, {c['at'][1]:.1f}], "
            f"levels: ({c['lo']}, {c['hi']}), note: {rust_str(c['note'])}, rare: {strs(c['rare'])}, own: {strs(c['own'])}, "
            f"table: {rust_str(c['table'])}, drops: {strs(c['drops'])} }},"
        )
    o.append("];")
    o.append("")
    o.append(f"pub const COACHES: [Coach; {len(coaches)}] = [")
    for c in coaches:
        o.append(f"    Coach {{ coachman: {rust_str(c['man'])}, town: {rust_str(c['town'])}, world: {c['world']}, at: [{c['at'][0]:.1f}, {c['at'][1]:.1f}], to: {strs(c['to'])} }},")
    o.append("];")
    o.append("")
    o.append("/// The towns, the biggest first.")
    o.append(f"pub const TOWNS: [Town; {len(towns)}] = [")
    for t in towns:
        o.append(f"    Town {{ name: {rust_str(t['name'])}, world: {t['world']}, at: [{t['at'][0]:.1f}, {t['at'][1]:.1f}], side: Side::{t['side'].title()} }}, // {t['size']} spawns about it")
    o.append("];")
    o.append("")
    o.append("/// The coach routes, each pair of towns once.")
    o.append(f"pub const ROUTES: [Route; {len(routes)}] = [")
    for (f, t), w, pa, pb in routes:
        o.append(f"    Route {{ from: {rust_str(f)}, to: {rust_str(t)}, world: {w}, a: [{pa[0]:.1f}, {pa[1]:.1f}], b: [{pb[0]:.1f}, {pb[1]:.1f}] }},")
    o.append("];")
    return "\n".join(o) + "\n"


if __name__ == "__main__":
    main()
