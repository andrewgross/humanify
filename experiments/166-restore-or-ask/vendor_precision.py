#!/usr/bin/env python3
"""exp166: vendor-placement precision — what names mode puts under vendor/,
judged against source-map truth, whether or not it was claimed.

Reads ONE run's diagnostics file (`--diagnostics`; the eraRestoration block
carries exp166's exact raw-to-era bridge: every era statement's RAW bundle
span, the freeze's moved spans, every vendored factory's raw span and file)
and the placement trail (the split's file per era statement), then reports:

  (a) app bytes placed under vendor/            (the bar is 0)
  (b) library bytes under the WRONG package's vendor folder
  (c) the old ordinal conversion's damage: converted region statements whose
      era statement holds a different raw statement, and era statements the
      freeze moved whose raw statement no claim of the moving package covers

Two vendor populations, reported apart:
  era      era statements the split placed under vendor/ (names mode's
           planner moves, and on a hop the placement's inheritance of them)
  factory  vendored CommonJS factories (the unpack; present in every mode)

A run without the bridge (main's logic) is judged with `--raw-from <diag>`:
the bridge spans of a bridged run of the SAME input. The era body and the
bridge do not depend on the placement decisions (the unpack's runtime text
and the formatter's output are the same), and the check REFUSES unless the
two runs' era statement spans are identical.

usage:
  python3 -I vendor_precision.py <diag.json> --truth bun-dev:<entry-id> [--raw-from <diag>]
  python3 -I vendor_precision.py <diag.json> --truth cc216 [--raw-from <diag>]
  python3 -I vendor_precision.py <diag.json> --truth none      # (c) only
Prints one JSON object.
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
    App bytes are spelled `<app>`; `<self>` is a copy inside the app's own
    package; `<unmapped>` the map never covered."""
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
            elif label.startswith("library:"):
                label = row.get("pkg") or label.split(":", 1)[1]
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


def sanitized(pkg):
    """The planner's path spelling of a package (sanitize_fs_path keeps
    scoped directories); compared loosely — case and separators."""
    return pkg.replace("\\", "/").strip("/").lower() if pkg else pkg


class Check:
    def __init__(self, diag, truth_at, families, raw_from=None):
        er = diag.get("eraRestoration") or {}
        self.er = er
        self.trails = {t["index"]: t for t in diag["placementTrails"]["trails"]}
        self.body_spans = [tuple(s) for s in er.get("bodySpans", [])]
        bridge = er
        if raw_from is not None:
            bridge = raw_from.get("eraRestoration") or {}
            theirs = [tuple(s) for s in bridge.get("bodySpans", [])]
            if theirs != self.body_spans:
                raise SystemExit("--raw-from: the two runs' era statement spans differ")
        self.raw = [tuple(r) if r else None for r in bridge.get("bodyRawSpans", [])]
        self.factories = [(tuple(s), f) for s, f in bridge.get("factoryFiles", [])]
        if "movedSpans" in er:
            self.moved = {tuple(s) for s in er["movedSpans"]}
        else:
            # main's logic froze every moved statement: the freeze's
            # bindings name them
            self.moved = {self.body_spans[st] for st, _, _ in er.get("frozen", [])}
        self.truth_at = truth_at
        self.fam = families

    def same(self, a, b):
        if self.fam is not None:
            return self.fam.same(a, b) or sanitized(a) == sanitized(b)
        return sanitized(a) == sanitized(b)

    def judge(self, population, path, span, out):
        """One vendor-placed raw span: its truth bytes by verdict."""
        pkg = vendor_package(path)
        if pkg is False:
            return
        n = span[1] - span[0]
        out[population + ":bytes"] += n
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

    def placement(self):
        out = collections.defaultdict(int)
        for k in ("era", "factory"):
            for s in ("files", "appFiles", "wrongFiles"):
                out[f"{k}:{s}"] = set()
        if not self.raw:
            out["noBridge"] = 1
            return out
        seen = set()
        for i, raw in enumerate(self.raw):
            path = self.trails.get(i, {}).get("file")
            if raw is None or (path, raw) in seen:
                continue  # a split statement shares its raw statement
            seen.add((path, raw))
            self.judge("era", path, raw, out)
        for span, path in self.factories:
            self.judge("factory", path, span, out)
        return out

    def conversion(self):
        """(c): the ordinal conversion against the exact bridge."""
        out = collections.Counter()
        claim_of = {}
        for region in self.er.get("regions", []):
            for st, raw, claim in zip(region["statements"], region["rawSpans"], region["claims"]):
                raw = tuple(raw)
                claim_of[raw] = claim
                out["regionStatements"] += 1
                if st < len(self.raw) and self.raw[st] == raw:
                    out["regionStatementsContentCorrect"] += 1
            out["regions"] += 1
        if not self.raw:
            return out
        for i, span in enumerate(self.body_spans):
            if span not in self.moved:
                continue
            path = self.trails.get(i, {}).get("file")
            pkg = vendor_package(path) or None
            raw = self.raw[i]
            claim = claim_of.get(raw)
            out["movedStatements"] += 1
            if claim is None or pkg is None or not self.same(pkg, claim):
                out["movedStatementsNotClaimedByMover"] += 1
                out["movedBytesNotClaimedByMover"] += (raw[1] - raw[0]) if raw else 0
        return out


def jsonable(d):
    return {k: (len(v) if isinstance(v, set) else v) for k, v in sorted(d.items())}


def run(diag_path, truth_spec, raw_from_path=None):
    with open(diag_path) as fh:
        diag = json.load(fh)
    raw_from = None
    if raw_from_path:
        with open(raw_from_path) as fh:
            raw_from = json.load(fh)
    families = None
    truth_at = None
    if truth_spec.startswith("bun-dev:"):
        truth_at = truth_bundev(truth_spec.split(":", 1)[1])
    elif truth_spec == "cc216":
        truth_at = truth_cc()
    if truth_at is not None:
        from families_lib import Families

        families = Families()
    check = Check(diag, truth_at, families, raw_from)
    placement = check.placement()
    lists = {
        "appFiles": sorted(placement["era:appFiles"] | placement["factory:appFiles"]),
        "wrongFiles": sorted(placement["era:wrongFiles"] | placement["factory:wrongFiles"]),
    }
    return {
        "placement": jsonable(placement),
        "conversion": dict(check.conversion()),
        **lists,
    }


def main():
    if len(sys.argv) < 3 or sys.argv[2] != "--truth":
        print(__doc__)
        sys.exit(2)
    raw_from = sys.argv[sys.argv.index("--raw-from") + 1] if "--raw-from" in sys.argv else None
    print(json.dumps(run(sys.argv[1], sys.argv[3], raw_from), indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
