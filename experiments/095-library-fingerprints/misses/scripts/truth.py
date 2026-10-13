"""exp095 "misses": ground truth per statement unit — the package (score_claims.py's majority-bytes label, so the
denominators match the indexv2 lane exactly), and for library units the package VERSION, the source file inside the
package and the original position the unit starts at (to find the same code in an index build).

usage: python3 -I truth.py heldout <slug> <bundle.js> <app package>      -> $MM/truth/<slug>.jsonl
       python3 -I truth.py wrangler                                        -> $MM/truth/wrangler.jsonl
       python3 -I truth.py cc                                              -> $MM/truth/cc-2.1.216.jsonl (labelled sample)
Versions: held-out = the package.json beside the mapped file in the app's install; wrangler = the pnpm path
(`.pnpm/<name>@<version>/`); Claude Code = the negatives lane's exact-version specs (exactlab/specs.txt)."""
import bisect
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mmlib import MM, load_dump  # noqa: E402

B64 = {c: i for i, c in enumerate("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/")}


def vlq(seg):
    out, val, shift = [], 0, 0
    for ch in seg:
        d = B64[ch]
        val += (d & 31) << shift
        if d & 32:
            shift += 5
        else:
            out.append(-(val >> 1) if val & 1 else val >> 1)
            val, shift = 0, 0
    return out


def decode(smap):
    """Per generated line: [(col, src, oline, ocol)]."""
    out, src, ol, oc = [], 0, 0, 0
    for group in smap["mappings"].split(";"):
        col, row = 0, []
        for seg in filter(None, group.split(",")):
            v = vlq(seg)
            col += v[0]
            if len(v) >= 4:
                src += v[1]
                ol += v[2]
                oc += v[3]
                row.append((col, src, ol, oc))
        out.append(row)
    return out


def segments(text, smap, inner=None):
    """(byte offset, src, oline, ocol), in order; through `inner` (map, line offset) when given."""
    lines = text.split("\n")
    starts = [0]
    for ln in lines[:-1]:
        starts.append(starts[-1] + len(ln.encode()) + 1)
    rows = decode(smap)
    if inner:
        ilines = decode(inner[0])

        def back(ol, oc):
            ol -= inner[1]
            if ol < 0 or ol >= len(ilines) or not ilines[ol]:
                return None
            r = ilines[ol]
            k = max(bisect.bisect_right(r, (oc, 1 << 62)) - 1, 0)
            return r[k][1:]
    out = []
    for li, row in enumerate(rows):
        line = lines[li] if li < len(lines) else ""
        ascii_line = line.isascii()
        for col, src, ol, oc in row:
            off = starts[li] + (col if ascii_line else len(line[:col].encode()))
            if inner:
                b = back(ol, oc)
                if b is None:
                    continue
                src, ol, oc = b
            out.append((off, src, ol, oc))
    return out


def package_root(source):
    """(package, path of the package root in the source string, file inside the package) or None."""
    if "node_modules/" not in source:
        return None
    head, rest = source.rsplit("node_modules/", 1)
    parts = rest.split("/")
    n = 2 if parts[0].startswith("@") else 1
    return "/".join(parts[:n]), head + "node_modules/" + "/".join(parts[:n]), "/".join(parts[n:])


def label_units(text, segs, sources, units, app_pkg, version_of):
    offs = [s[0] for s in segs]
    out = []
    for u in units:
        if not (u["k"] in ("Statement", "LazyBody") and not u["rt"]):
            continue
        s, e = u["s"], u["e"]
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes, files, first = collections.Counter(), collections.Counter(), {}
        while k < len(segs) and segs[k][0] < e:
            a, b = max(segs[k][0], s), min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                src = sources[segs[k][1]]
                pr = package_root(src)
                p = pr[0] if pr else None
                votes["<app>" if p is None else ("<self>" if p == app_pkg else p)] += b - a
                files[segs[k][1]] += b - a
                first.setdefault(segs[k][1], segs[k][2:])
            k += 1
        lab = votes.most_common(1)[0][0] if votes else "<unmapped>"
        row = {"i": u["i"], "s": s, "e": e, "b": u["b"], "label": lab}
        if lab not in ("<app>", "<self>", "<unmapped>"):
            src = next(si for si, _ in files.most_common() if (package_root(sources[si]) or ("",))[0] == lab)
            pkg, root, f = package_root(sources[src])
            row.update(pkg=pkg, version=version_of(root, pkg), file=f, pos=list(first[src]))
        out.append(row)
    return out


def write(name, rows):
    os.makedirs(f"{MM}/truth", exist_ok=True)
    with open(f"{MM}/truth/{name}.jsonl", "w") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")


def heldout(slug, js, app):
    text = open(js, encoding="utf8").read()
    smap = json.load(open(js + ".map"))
    base = os.path.dirname(js)
    cache = {}

    def version_of(root, pkg):
        p = os.path.normpath(os.path.join(base, root, "package.json"))
        if p not in cache:
            try:
                cache[p] = json.load(open(p)).get("version")
            except (OSError, ValueError):
                cache[p] = None
        return cache[p]
    units = load_dump(f"{MM}/dump/{slug}.jsonl")
    write(slug, label_units(text, segments(text, smap), smap["sources"], units, None if app == "-" else app,
                            version_of))


def wrangler():
    d = "/work/exp095-index-v2-data/eval/wrangler"
    text = open(f"{d}/cli.min.js", encoding="utf8").read()
    outer, inner = json.load(open(f"{d}/cli.min.js.map")), json.load(open(f"{d}/cli.js.map"))

    def version_of(root, pkg):
        m = re.search(r"\.pnpm/([^/]+)/node_modules/", root)
        if not m:
            return None
        at = m.group(1).replace("+", "/")
        at = at.split("_")[0]  # pnpm peer suffix
        return at.rsplit("@", 1)[1] if "@" in at[1:] else None
    units = load_dump(f"{MM}/dump/wrangler.jsonl")
    write("wrangler", label_units(text, segments(text, outer, (inner, 1)), inner["sources"], units, None, version_of))


def cc():
    neg = "/work/exp095-library-fingerprints/negatives"
    specs = {}
    for line in open(f"{neg}/exactlab/specs.txt"):
        if line.startswith("#") or "|" not in line:
            continue
        for spec in line.split("|")[1].split():
            name, ver = spec.rsplit("@", 1)
            specs.setdefault(name, ver)
    meta = json.load(open(f"{neg}/data/sample-meta.json"))
    units = {u["s"]: u for u in load_dump(f"{MM}/dump/cc-2.1.216.jsonl")}
    rows = []
    for line in open(f"{neg}/labels.jsonl"):
        r = json.loads(line)
        if r["stratum"] not in meta["strata"]:
            continue
        u = units.get(r["start"])
        lab = r["label"]
        row = {"i": u["i"] if u else None, "s": r["start"], "e": r["end"], "b": r["bytes"], "stratum": r["stratum"],
               "label": lab.split(":", 1)[1] if lab.startswith("library:") else "<" + lab + ">",
               "unitMatches": bool(u and u["e"] == r["end"])}
        if lab.startswith("library:"):
            row.update(pkg=r["pkg"], version=r.get("version_if_known") or specs.get(r["pkg"]), file=None, pos=None)
        rows.append(row)
    write("cc-2.1.216", rows)


if __name__ == "__main__":
    if sys.argv[1] == "heldout":
        heldout(*sys.argv[2:5])
    elif sys.argv[1] == "wrangler":
        wrangler()
    else:
        cc()
