#!/usr/bin/env python3
"""exp166: vendor-placement precision — what names mode puts under vendor/,
judged against source-map truth, whether or not it was claimed.

Reads ONE names-mode run's diagnostics file (`--diagnostics`). Its
eraRestoration block carries exp166's exact raw-to-era bridge (every era
statement's RAW bundle span), the moved modules (path + era statement
spans; the split pre-fills them, so no placement trail records them), the
vendored factories (raw span + file), and — diagnostics only — the plan the
RETIRED ordinal conversion would have produced from the same claimed groups.
So one run judges two worlds over the identical era body:

  exact    what this binary placed (the bridge's conversion)
  ordinal  what main's logic placed (the `i - removed_before` conversion)

and reports, per world:

  (a) app bytes placed under vendor/                      (the bar is 0)
  (b) library bytes under the WRONG package's vendor folder
  (c) the conversion's damage: converted region statements whose era
      statement holds a different raw statement; moved statements whose raw
      statement no claim of the moving package covers (frozen wholesale in
      main's logic)

Populations: `era` (moved modules + any other era statement the split placed
under vendor/) and `factory` (vendored CommonJS factories — the unpack, the
same in both worlds and in every mode).

usage:
  python3 -I vendor_precision.py <diag.json> --truth bun-dev:<entry-id>|cc216|none
Prints one JSON object. (`npm run eval -- vendor-precision check …`)
"""
import bisect
import collections
import gzip
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SCORER = os.path.join(HERE, "../096-library-matching-score")
sys.path.insert(0, SCORER)  # python3 -I ignores PYTHONPATH

CC_TRUTH = "/work/cc-library-truth/2.1.216/labels.jsonl"
NOT_LIBRARY = {"<app>", "<self>", "<unmapped>", "<foreign>"}


# ---------------------------------------------------------------- truth ---
def truth_bundev(entry_id):
    """Per raw span: {label: bytes} from the frozen segment table (lib096).
    App bytes are spelled `<app>` (NOT `app` — exp165's census lumped them
    into "other"); `<self>` is a copy inside the app's own package."""
    import lib096

    lib096.use("bun-dev")
    entries = {
        e["id"]: e
        for e in json.load(open(os.path.join(SCORER, "corpus-bun-dev.json")))["entries"]
    }
    with gzip.open(lib096.truth_path(entries[entry_id]), "rt") as fh:
        t = json.load(fh)
    segs = t["segments"]
    srcs = [s[0] for s in t["sources"]]
    offs = [s[0] for s in segs]

    def at(s, e):
        votes = collections.Counter()
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        while k < len(segs) and segs[k][0] < e:
            a = max(segs[k][0], s)
            b = min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                votes[srcs[segs[k][1]]] += b - a
            k += 1
        return votes

    return at


def truth_cc():
    """The 620-unit labelled sample on 2.1.216: per raw span, the labelled
    units INSIDE it (most of the bundle is unlabelled — the sample only)."""
    units = []
    with open(CC_TRUTH) as fh:
        for line in fh:
            row = json.loads(line)
            label = row["label"]
            if label == "app":
                label = "<app>"
            elif label.startswith("library"):
                label = row.get("pkg") or label.split(":", 1)[-1]
            else:
                label = "<" + label + ">"
            units.append((row["start"], row["end"], label))
    units.sort()
    starts = [u[0] for u in units]

    def at(s, e):
        votes = collections.Counter()
        k = bisect.bisect_left(starts, s)
        while k < len(units) and units[k][0] < e:
            us, ue, label = units[k]
            if ue <= e:
                votes[label] += ue - us
            k += 1
        return votes

    return at


# ------------------------------------------------------------- the check ---
def vendor_package(path):
    """`vendor/<pkg>/…` → pkg (scoped keeps its scope); a file at the vendor
    root has no package folder (None); not under vendor/ → False."""
    if not path or not path.startswith("vendor/"):
        return False
    parts = path[len("vendor/"):].split("/")
    if len(parts) < 2:
        return None
    if parts[0].startswith("@") and len(parts) >= 3:
        return parts[0] + "/" + parts[1]
    return parts[0]


def loose(pkg):
    return pkg.replace("\\", "/").strip("/").lower() if pkg else pkg


class Check:
    def __init__(self, diag, truth_at, families):
        er = diag.get("eraRestoration") or {}
        self.er = er
        self.trails = {t["index"]: t for t in diag["placementTrails"]["trails"]}
        self.body_spans = [tuple(s) for s in er.get("bodySpans", [])]
        self.index_of = {s: i for i, s in enumerate(self.body_spans)}
        self.raw = [tuple(r) if r else None for r in er.get("bodyRawSpans", [])]
        self.factories = [(tuple(s), f) for s, f in er.get("factoryFiles", [])]
        self.truth_at = truth_at
        self.fam = families

    def same(self, a, b):
        if loose(a) == loose(b):
            return True
        return self.fam is not None and self.fam.same(a, b)

    def judge(self, population, path, span, out):
        """One vendor-placed raw span: its truth bytes by verdict."""
        pkg = vendor_package(path)
        if pkg is False or span is None:
            return
        out[population + ":bytes"] += span[1] - span[0]
        out[population + ":files"].add(path)
        if self.truth_at is None:
            return
        votes = self.truth_at(*span)
        for label, b in votes.items():
            if label == "<app>":
                out[population + ":app"] += b
                out[population + ":appFiles"].add(path)
            elif label == "<self>":
                out[population + ":self"] += b
            elif label in NOT_LIBRARY or label.startswith("<"):
                out[population + ":other"] += b
            elif pkg is None:
                out[population + ":libraryAtRoot"] += b
            elif self.same(pkg, label):
                out[population + ":libraryRight"] += b
            else:
                out[population + ":libraryWrongPackage"] += b
                out[population + ":wrongFiles"].add(path)
        out[population + ":labelled"] += sum(votes.values())

    def moved_statements(self, files):
        """{era index: path} for one world's moved modules."""
        out = {}
        for path, spans in files:
            for span in spans:
                i = self.index_of.get(tuple(span))
                if i is not None:
                    out[i] = path
        return out

    def world(self, regions, moved_files):
        out = collections.defaultdict(int)
        for s in ("files", "appFiles", "wrongFiles"):
            out["era:" + s] = set()
        moved = self.moved_statements(moved_files)
        both = set(self.moved_statements(self.er.get("movedFiles", [])))
        both |= set(self.moved_statements(self.er.get("ordinalMovedFiles", [])))
        seen = set()
        # (a)/(b): the moved modules, then any other statement the split
        # put under vendor/ (the same in both worlds)
        placed = list(moved.items()) + [
            (i, t.get("file")) for i, t in self.trails.items() if i not in both
        ]
        for i, path in placed:
            raw = self.raw[i] if i < len(self.raw) else None
            if (path, raw) in seen:
                continue  # a split statement shares its raw statement
            seen.add((path, raw))
            self.judge("era", path, raw, out)
        # (c)
        claim_of = {}
        for region in regions:
            out["regions"] += 1
            for st, raw, claim in zip(region["statements"], region["rawSpans"], region["claims"]):
                raw = tuple(raw)
                claim_of[raw] = claim
                out["regionStatements"] += 1
                if st < len(self.raw) and self.raw[st] == raw:
                    out["regionStatementsContentCorrect"] += 1
        for i, path in moved.items():
            raw = self.raw[i] if i < len(self.raw) else None
            claim = claim_of.get(raw)
            out["movedStatements"] += 1
            out["movedBytes"] += (raw[1] - raw[0]) if raw else 0
            pkg = vendor_package(path) or None
            if claim is None or pkg is None or not self.same(pkg, claim):
                out["movedStatementsNotClaimedByMover"] += 1
                out["movedBytesNotClaimedByMover"] += (raw[1] - raw[0]) if raw else 0
        out["movedModules"] = len(moved_files)
        return out

    def factory(self):
        out = collections.defaultdict(int)
        for s in ("files", "appFiles", "wrongFiles"):
            out["factory:" + s] = set()
        for span, path in self.factories:
            self.judge("factory", path, span, out)
        return out


def jsonable(d):
    return {k: (sorted(v) if isinstance(v, set) else v) for k, v in sorted(d.items())}


def run(diag_path, truth_spec):
    with open(diag_path) as fh:
        diag = json.load(fh)
    er = diag.get("eraRestoration") or {}
    if "bodyRawSpans" not in er:
        return {"noBridge": 1}
    families = None
    truth_at = None
    if truth_spec.startswith("bun-dev:"):
        truth_at = truth_bundev(truth_spec.split(":", 1)[1])
    elif truth_spec == "cc216":
        truth_at = truth_cc()
    if truth_at is not None:
        from families_lib import Families

        families = Families()
    check = Check(diag, truth_at, families)
    return {
        "exact": jsonable(check.world(er.get("regions", []), er.get("movedFiles", []))),
        "ordinal": jsonable(
            check.world(er.get("ordinalRegions", []), er.get("ordinalMovedFiles", []))
        ),
        "factory": jsonable(check.factory()),
        "conversion": er.get("conversion"),
    }


def main():
    if len(sys.argv) < 4 or sys.argv[2] != "--truth":
        print(__doc__)
        sys.exit(2)
    print(json.dumps(run(sys.argv[1], sys.argv[3]), indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
