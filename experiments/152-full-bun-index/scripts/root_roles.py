"""exp152: the installed index lists every version as `pinned` (exp142 merged them all as --pinned), so delta_plan.py
applies the pins' policy (year-last12+major4) to every package and the roots' year-all+minor role is lost. This
estimates what that costs: a package whose indexed versions (plus the ones the index refused) hold every version
year-all+minor picks at the OLD as-of looks like a root; count what year-all+minor would add at the new as-of on top
of the delta. Report only; the build uses delta_plan.py's list.

usage: python3 -I root_roles.py <index dir> <registry dir> <new as-of YYYY-MM-DD> <delta pins.txt>"""
import datetime
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../142-index-coverage/scripts"))
sys.path.insert(0, os.path.join(HERE, "../../101-index-coverage/scripts"))
import delta_plan  # noqa: E402


def main():
    ix, reg, new, delta = sys.argv[1:5]
    m = json.load(open(os.path.join(ix, "manifest.json")))
    old = datetime.datetime.fromisoformat(m["asOf"].replace("Z", "+00:00"))
    new = datetime.datetime.fromisoformat(new + "T23:59:59.999+00:00")
    held = {}
    for p in m["packages"] + m["refused"]:
        held.setdefault(p["name"], set()).add(p["version"])
    have = {l.strip() for l in open(delta) if l.strip()}
    roots, extra = 0, 0
    for name in sorted({p["name"] for p in m["packages"]}):
        doc = json.load(open(os.path.join(reg, name.replace("/", "__") + ".json")))
        if not delta_plan.year_all_minor(doc, old) <= held[name]:
            continue
        roots += 1
        times = {v: datetime.datetime.fromisoformat(t.replace("Z", "+00:00")) for v, t in doc["time"].items()}
        extra += sum(1 for v in delta_plan.year_all_minor(doc, new) - held[name]
                     if times[v] > old and f"{name}@{v}" not in have)
    print(json.dumps({"rootLike": roots, "extraVersionsIfRoots": extra}))


if __name__ == "__main__":
    main()
