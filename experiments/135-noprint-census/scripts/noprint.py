"""exp135: WHY the truth package has no print of a wrong seed's unit in the index (exp134's largest wrong class,
"truth family not read for the unit"), classified from the index's own build artefacts.

For every wrong-family index seed of one label (exp133's judge13 rows, slot c0/c1) that exp134's census puts in
`not-held` (the truth family holds no print of the unit at all) or `filtered` (holds one the lookup does not read),
take the truth row scorer 13 gives the unit (label package T, version V, file F, origin O) and ask the index:
  a  not-indexed        T is not in the index at all (versions.json)
  b  other-version      T is indexed, V is not: nearest indexed version by semver (patch/minor/major step) and
                        by release date (registry packument); whether F is a built file at any indexed version
  c  dropped            T@V is indexed, the unit is not in its rows; read from T@V's shard (store/shards/<slice>):
                          slice-missing     no shard of T@V in the bundle's slice
                          entry-not-built   F is not a module of the shard (file in the installed package but
                                            unreachable from the package's main entry, which is all the builder
                                            bundles) -- `file-absent` when F is not in the package either
                          sponge            F's rows dropped by the sponge rule (notes.spongeUnitFiles)
                          no-rows-other     F is a module, no unit rows for it, no sponge note (unit floor etc.)
  d  print-differs      F has unit rows at T@V but none carries the unit's prints (normalisation / shape gap);
                        `other-slice` when another slice's shard of T@V does carry them
  f  filtered           T's family holds a print of the unit; the lookup does not read it (specificity / owners)
  e  truth-questionable the truth row is not a plain map label: misattributed (scorer 13), copy/chunk origin, or
                        the labelled version is not installed under that name (no package.json match)
Classes are judged on the label package T; an origin O (when the truth has one) is recorded as `origin-indexed`
yes/no beside it (the claim would be right for either).

usage: <venv python> -I noprint.py <corpus> <judge.jsonl|-> <slot c0|c1> <label> <out-prefix> [--private]
                                   [--store <store/shards> --work <work dir>]   (default: exp130's, k2-all-r5-origin)
  `-` judges the label inline (judge13's verdicts); the only way it reads a frozen test split.
  writes <out-prefix>.rows.jsonl (per unit; package names withheld for the held-out set and for --private
  corpora) and prints per-set aggregates (KB per class/subclass). --private (test only): aggregates only."""
import collections
import datetime
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "../../096-library-matching-score"))
sys.path.insert(0, os.path.join(HERE, "../../134-carrier-ownership/scripts"))
import lib096  # noqa: E402

STORE = "/work/exp130-data/full/store/shards"
WORK = "/work/exp130-data/full/work"
REGISTRIES = ("/work/exp130-data/full/registry", "/work/exp118-data/idx/registry", "/work/exp133-data/registry")
SLICES = ("bun-bun-cjs-min", "bun-node-cjs-min", "esbuild-node-cjs-min", "rollup-node-cjs-min",
          "webpack-node-cjs-min")
HIDDEN = ("heldout",)


def esc(p):
    return p.replace("/", "__")


def semver(v):
    core = v.split("+")[0]
    pre = "-" in core
    nums = core.split("-")[0].split(".")
    try:
        return tuple(int(x) for x in nums[:3]) + (0 if pre else 1,)
    except ValueError:
        return None


_REG = {}


def times(p):
    if p not in _REG:
        _REG[p] = {}
        for r in REGISTRIES:
            f = os.path.join(r, esc(p) + ".json")
            if os.path.exists(f):
                try:
                    _REG[p] = json.load(open(f)).get("time") or {}
                except ValueError:
                    continue
                break
    return _REG[p]


def day(s):
    try:
        return datetime.datetime.fromisoformat(s.replace("Z", "+00:00"))
    except (ValueError, AttributeError):
        return None


def nearest(p, v, indexed):
    """(semver step to the nearest indexed version, |days| to the nearest indexed version by date, newer/older)."""
    sv = semver(v)
    step = "unparsed"
    if sv:
        best = None
        for x in indexed:
            sx = semver(x)
            if not sx:
                continue
            d = (0 if sx[0] == sv[0] else 1, 0 if sx[:2] == sv[:2] else 1, abs(sx[2] - sv[2]))
            best = d if best is None or d < best else best
        if best:
            step = "major" if best[0] else "minor" if best[1] else "patch"
    t = times(p)
    tv = day(t.get(v))
    gap = None
    side = "?"
    if tv:
        ds = [(abs((day(t[x]) - tv).days), x) for x in indexed if day(t.get(x))]
        if ds:
            gap, x = min(ds)
            side = "indexed-newer" if day(t[x]) > tv else "indexed-older"
    return step, gap, side


_SHARD = {}


def shard(sl, p, v):
    k = (sl, p, v)
    if k not in _SHARD:
        f = os.path.join(STORE, sl, f"{esc(p)}@{v}.json")
        _SHARD[k] = json.load(open(f)) if os.path.exists(f) else None
        if len(_SHARD) > 4000:
            _SHARD.clear()
            _SHARD[k] = json.load(open(f)) if os.path.exists(f) else None
    return _SHARD[k]


def unit_hashes(u):
    return {u["w"][0]} | {f[2] for f in u["f"] if f[3] >= 30}


VENDORED = ("vendor", "vendored", "third_party", "third-party")
SOURCE_EXT = (".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".json")


def marker_path(line):
    """build.rs module_marker(): the path a bundler module marker line names, or None."""
    t = line.strip()
    if t.startswith("//#region "):
        return t[10:].strip() or None
    if t.startswith("/*!*** ") and t.endswith("***!*/"):
        return t[7:-6].strip() or None
    if t.startswith("/***/ ") and t.endswith(":"):
        r = t[6:-1].strip()
        return r[1:-1] if len(r) > 1 and r[0] == r[-1] and r[0] in "'\"" else None
    if t.startswith("// "):
        r = t[3:].strip()
        return r if r and not any(c.isspace() for c in r) and r.endswith(SOURCE_EXT) else None
    return None


def foreign_marker(p, v, f):
    """Does T@V's installed F carry a module marker naming ANOTHER package's node_modules path (build.rs
    sponge_evidence's Regions)?"""
    path = os.path.join(WORK, f"{esc(p)}@{v}", "node_modules", p, f)
    try:
        if os.path.getsize(path) > 64 << 20:
            return False
        text = open(path, encoding="utf8", errors="replace").read()
    except OSError:
        return False
    for line in text.splitlines():
        m = marker_path(line)
        if m and "node_modules/" in m:
            rest = m.rsplit("node_modules/", 1)[1].split("/")
            pkg = "/".join(rest[:2]) if rest[0].startswith("@") else rest[0]
            if pkg and pkg != p:
                return True
    return False


def vendored(f):
    parts = f.split("/")
    return any(x in VENDORED for x in parts[:-1]) or parts[-1].startswith("third-party.")


def build_error(failed):
    """A short kind for a failed slice build's error text (no paths)."""
    e = " ".join(failed.values())
    m = re.search(r'No loader is configured for "(\.[A-Za-z0-9]+)"', e)
    if m:
        return "no-loader" + m.group(1)
    if "Could not resolve" in e or "Cannot find module" in e or "Could not find" in e:
        return "unresolved-import"
    if "timed out" in e or "timeout" in e.lower():
        return "timeout"
    return "other"


DL25K = "/work/exp133-data/full/ix/k2-all-r5-origin-dl25k/versions.json"
AS_OF = datetime.datetime(2026, 10, 11, 23, 59, 59, tzinfo=datetime.timezone.utc)
_DL = {}


def dl25k():
    if not _DL:
        _DL.update(json.load(open(DL25K)))
    return _DL


def policies(p, v):
    """Would a version POLICY have indexed V? latestPatchOfMinor: V is the newest patch of its minor published by
    the as-of; lastYear: V was published in the 12 months before it; latestOfMajor: the newest of its major."""
    t = {x: day(y) for x, y in times(p).items() if semver(x) and day(y) and day(y) <= AS_OF}
    sv = semver(v)
    if v not in t or not sv:
        return {"policyKnown": False}
    same_minor = [x for x in t if semver(x)[:2] == sv[:2] and semver(x)[3] == 1]
    same_major = [x for x in t if semver(x)[0] == sv[0] and semver(x)[3] == 1]
    return {"policyKnown": True,
            "latestPatchOfMinor": bool(same_minor) and max(same_minor, key=semver) == v,
            "latestOfMajor": bool(same_major) and max(same_major, key=semver) == v,
            "lastYear": (AS_OF - t[v]).days <= 365}


def filtered_why(ix, u, hs, fam, win, mmlib):
    """Why the lookup does not read the truth family's print: spec-bound (every row of it sits in a module over
    the specificity bound: the family holds the code only in a much bigger file), too-many-owners (more than
    max_packages hold the print), whole-read-first (the lookup reads the whole print, which the winner holds and
    the family does not; the family holds only function prints)."""
    spec = mmlib.RULES["spec"]
    lim = spec[0] * u["mb"] + spec[1]
    rows = [(h, r) for h in hs for r in ix.units.get(h, ()) if fam(r[0])]
    if rows and all(r[3] > lim for _, r in rows):
        return "spec-bound"
    whole = u["w"][0]
    wown = {r[0] for r in ix.units.get(whole, ()) if r[3] <= lim}
    if win in wown and not any(fam(o) for o in wown):
        return "whole-read-first"
    for h, r in rows:
        if r[3] <= lim and len({x[0] for x in ix.units.get(h, ()) if x[3] <= lim}) > mmlib.RULES["max_packages"]:
            return "too-many-owners"
    return "other"


UNIT_NODES = [0]  # the current unit's whole-print nodes (read by classify_dropped)


def classify_dropped(sl, p, v, f, hs):
    """(class, sub) for an indexed T@V whose rows do not carry the unit."""
    sh = shard(sl, p, v)
    if sh is None or sh.get("units") is None:
        built = [o for o in SLICES if o != sl and shard(o, p, v) is not None]
        held = [o for o in built if hs & {r["hash"] for r in shard(o, p, v).get("units", ()) if r["owner"] == p}]
        return "c-dropped", "slice-missing" + ("-other-slice-holds" if held else "-other-built" if built else "")
    builds = (sh.get("notes") or {}).get("builds") or {}
    failed = {k: e for k, e in builds.items() if e != "ok"}
    if builds and len(failed) == len(builds):
        held = [o for o in SLICES if o != sl and shard(o, p, v) is not None
                and hs & {r["hash"] for r in shard(o, p, v).get("units", ()) if r["owner"] == p}]
        return "c-dropped", "build-failed-" + build_error(failed) + ("-other-slice-holds" if held else "")
    every = [shard(o, p, v) for o in SLICES]
    mods = {m["file"] for d in every if d for m in d.get("modules", ()) if m["owner"] == p}
    mods |= {r["file"] for d in every if d for r in d.get("units", ()) if r["owner"] == p}
    rows = [r for r in sh.get("units", ()) if r["owner"] == p and r["file"] == f]
    reowned = hs & {r["hash"] for r in sh.get("units", ()) if r["owner"] != p}
    if rows:
        if hs & {r["hash"] for r in rows}:
            return "f-filtered", "held-at-version"
        if reowned:
            return "c-dropped", "sponge-region-reowned"
        if foreign_marker(p, v, f):
            return "c-dropped", "sponge-region-dropped"
        for other in SLICES:
            o = shard(other, p, v) if other != sl else None
            if o and hs & {r["hash"] for r in o.get("units", ()) if r["owner"] == p}:
                return "d-print-differs", "other-slice"
        sizes = {r["nodes"] for r in rows}
        return "d-print-differs", "same-file-rows" + ("-same-size" if UNIT_NODES[0] in sizes else "")
    if f in mods:
        slice_has = any(m["owner"] == p and m["file"] == f for m in sh.get("modules", ()))
        other_rows = [o for o in SLICES if o != sl and shard(o, p, v) is not None
                      and any(r["owner"] == p and r["file"] == f for r in shard(o, p, v).get("units", ()))]
        if not vendored(f) and not reowned and other_rows and (sh.get("notes") or {}).get("multiModuleFiles"):
            held = [o for o in other_rows if hs & {r["hash"] for r in shard(o, p, v)["units"]
                                                   if r["owner"] == p and r["file"] == f}]
            return "c-dropped", ("multi-module-dropped" + ("-module-kept" if slice_has else "")
                                 + ("-other-slice-holds" if held else ""))
        if vendored(f):
            return "c-dropped", "sponge-vendored-path"
        if reowned:
            return "c-dropped", "sponge-region-reowned"
        if foreign_marker(p, v, f):
            return "c-dropped", "sponge-marker"
        mm = bool((sh.get("notes") or {}).get("multiModuleFiles"))
        return "c-dropped", f"no-rows-other-mod{int(slice_has)}-oth{int(bool(other_rows))}-mm{int(mm)}"
    present = os.path.exists(os.path.join(WORK, f"{esc(p)}@{v}", "node_modules", p, f))
    if not present:
        return "c-dropped", "file-absent"
    return "c-dropped", "entry-not-built" + ("-vendored" if vendored(f) else "")


def inline_rows(rep, units, lab, key, entry, truth, score, slot):
    """judge13's per-claim verdict for one label, kept to the wrong-family index seeds."""
    scope = {u["i"]: "statement" for u in score.statement_units(units)}
    scope.update({u["i"]: "factory" for u in score.factory_units(units)})
    out = []
    for c in rep.get("claims", []):
        u = key.get((c["start"], c["end"]))
        if u is None:
            continue
        ev = (c.get("evidence") or {}).get("kind", "")
        if c["rule"] != "seed" or ev not in ("index", "queueIndex"):
            continue
        if score.judge_origin(c["package"], lab[u["i"]], truth["appPackage"])[0] != "wrongFamily":
            continue
        out.append({"set": entry["set"], "start": u["s"], "end": u["e"], "bytes": u["b"], "scope": scope[u["i"]],
                    "truth": lab[u["i"]]["label"], "origin": lab[u["i"]].get("origin"),
                    slot: {"package": c["package"], "rule": "seed", "evidence": ev, "verdict": "wrongFamily"}})
    return out


def main():
    corpus, path, slot, label, prefix = sys.argv[1:6]
    private = "--private" in sys.argv
    global STORE, WORK
    if "--store" in sys.argv:  # the shard store / work dir the label's index was built from
        STORE = sys.argv[sys.argv.index("--store") + 1]
        WORK = sys.argv[sys.argv.index("--work") + 1]
    if corpus == "test2":
        raise SystemExit("REFUSED: test2 is a frozen judge, never diagnosed")
    if corpus in lib096.PRIVATE and not private:
        raise SystemExit("REFUSED: a test split only with --private (aggregates only)")
    lib096.use(corpus)
    import census
    import mmlib
    import score
    import truth_audit
    same = score.same_family
    om = lib096.load_origins_manifest()
    cm = truth_audit.load_copies_manifest()
    chm = truth_audit.load_chained_manifest()
    o7m = truth_audit.load_origins7_manifest()
    wrong = collections.defaultdict(list)
    for line in open(path) if path != "-" else ():
        r = json.loads(line)
        c = r.get(slot)
        if c and c["verdict"] == "wrongFamily" and c["rule"] == "seed" and c["evidence"] in ("index", "queueIndex"):
            wrong[r["entry"]].append(r)
    entries = {e["id"]: e for e in lib096.load_manifest()["entries"]}
    if path == "-":  # inline judge (judge13's verdicts, one label): the frozen test split, never written out
        wrong = {eid: None for eid, e in entries.items() if (e.get("truth") or {}).get("method") == "sourcemap"
                 and os.path.exists(os.path.join(lib096.DATA, "results", label, "reports", eid + ".json"))}
    pipe = json.load(open(os.path.join(lib096.DATA, "results", label, "pipeline.json")))
    ixs = {}
    agg = collections.Counter()
    seen_total = collections.Counter()
    out = None if private else open(prefix + ".rows.jsonl", "w")
    for eid, rs in sorted(wrong.items()):
        entry = entries[eid]
        truth = truth_audit.patch_sources(entry, lib096.load_truth(entry), chm)
        origins = truth_audit.origin_layer(entry, om, o7m)
        truth, origins = truth_audit.redecode(entry, truth, origins)
        audit = truth_audit.Audit(entry, truth, cm)
        doc = json.load(open(os.path.join(lib096.DATA, "results", label, "reports", eid + ".json")))
        sl = doc["toolchain"]["slice"]
        if sl not in ixs:
            ixs[sl] = score.load_index(pipe["indexDir"], sl, pipe["indexId"])
        ix = ixs[sl]
        if not (doc.get("libraryDiscovery") or {}).get("unitRows"):
            continue
        units = score.run_units(doc["libraryDiscovery"])
        lab = score.label_units(truth, units, origins, audit)
        key = {(u["s"], u["e"]): u for u in score.scored_units(units)}
        if rs is None:
            if truth["method"] != "sourcemap" or not (doc.get("libraryDiscovery") or {}).get("unitRows"):
                continue
            rs = inline_rows(doc["libraryDiscovery"], units, lab, key, entry, truth, score, slot)
        for r in rs:
            u = key.get((r["start"], r["end"]))
            seen_total[r["set"]] += r["bytes"]
            if u is None or "w" not in u:
                continue
            row = lab[u["i"]]
            right = [x for x in (r["truth"], r.get("origin")) if x and not x.startswith("<")]
            sc = census.scores(ix, u, mmlib)
            win = r[slot]["package"]
            if win not in sc or any(same(p, t) for p in sc for t in right):
                continue  # reassigned / tie / outscored: not this class
            fam = lambda p: any(same(p, t) for t in right)  # noqa: E731
            held = census.holds_any(ix, u, fam)
            T, V, F, O = row.get("pkg"), row.get("version"), row.get("file"), row.get("origin")
            hs = unit_hashes(u)
            extra = {}
            if T is None:
                cls, sub = "e-truth-questionable", "no-library-label"
            elif "misattributedFrom" in row:
                cls, sub = "e-truth-questionable", "misattributed"
            elif held:
                cls, sub = "f-filtered", filtered_why(ix, u, hs, fam, win, mmlib)
            elif not V:
                cls, sub = "e-truth-questionable", "no-version-in-map"
            elif not ix.has_package(T):
                dl = dl25k()
                extra = {"inDl25k": T in dl, "inDl25kVersion": V in dl.get(T, ())}
                cls = "a-not-indexed"
                sub = ("in-dl25k-at-version" if extra["inDl25kVersion"] else "in-dl25k-other-version"
                       if extra["inDl25k"] else "not-in-dl25k")
            elif not ix.has_version(T, V):
                step, gap, side = nearest(T, V, ix.versions[T])
                built = bool(ix.file_versions(T, F))
                extra = {"gapDays": gap, "side": side, "fileBuiltElsewhere": built, **policies(T, V)}
                cls = "b-other-version"
                sub = ("after-as-of" if not extra["policyKnown"] else "last-year" if extra["lastYear"]
                       else "latest-patch-of-minor" if extra["latestPatchOfMinor"] else "older") + "/" + step
            else:
                UNIT_NODES[0] = u["w"][1]
                cls, sub = classify_dropped(sl, T, V, F, hs)
            wfiles = {r[1] for h in hs for r in ix.units.get(h, ()) if r[0] == win}
            extra["claimerSameFile"] = F in wfiles
            extra["claimerSameBase"] = bool(F) and os.path.basename(F) in {os.path.basename(x) for x in wfiles}
            extra["kind"] = u["k"]
            if O and O != T:
                extra["originIndexed"] = ix.has_package(O)
                extra["originEvidence"] = row.get("originEvidence")
            agg[(r["set"], cls, sub)] += r["bytes"]
            if out:
                hide = r["set"] in HIDDEN
                rec = {"entry": None if hide else eid, "set": r["set"], "scope": r["scope"], "bytes": r["bytes"],
                       "cls": cls, "sub": sub, "slice": sl, "nodes": u["w"][1], "mb": u["mb"], **extra}
                if not hide:
                    rec.update(truth=T, version=V, file=F, origin=O, claimer=win, hashes=sorted(hs),
                               indexedVersions=len(ix.versions.get(T, ())))
                out.write(json.dumps(rec) + "\n")
    print(f"# {label} ({slot}) {corpus}: wrong index-seed KB per set: "
          + ", ".join(f"{k} {v / 1e3:.1f}" for k, v in sorted(seen_total.items())))
    for k, b in sorted(agg.items(), key=lambda kv: (kv[0][0], kv[0][1], -kv[1])):
        print(f"{b / 1e3:10.1f} KB  " + "  ".join(k))


if __name__ == "__main__":
    main()
