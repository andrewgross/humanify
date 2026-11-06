#!/usr/bin/env python3
"""exp190 (c): tree-shaking survival and index matching per app x bundler.

For each bundle (apps.py) the unit prints come from `humanify-ag discover
--unit-prints --bundler <b>` (the run's own unit model for that bundler);
each print (whole statement or function, >= 20 nodes) is attributed through
the bundle's sourcemap to the source file owning the majority of the mapping
segments inside its span (a print whose segments are < 90% one file is
MIXED and not attributed — the index builder's rule). A print attributed to
node_modules/<P>/ is P's SURVIVING code in that bundle.

Per app x bundler:
  survivors     P's prints in the bundle
  matchAny      survivors whose hash the index holds under owner P (any version, either Bun slice)
  matchVersion  ... under P at the installed version
  indexedFiles  P's files with a row at that version; survivingFiles: of those, files with a survivor
  survivedRows  P@V's indexed rows whose hash is anywhere in the bundle (the
                hash view of survival; only meaningful where the bundler
                prints what the index recorded)
Unmatched survivors are paired with the bun bundle's print of the same kind
starting on the same ORIGINAL line (source file + line of the print's first
mapped segment; nearest column, then node count) and, when bun's print
matched the index, the pair's texts are classified (see `transform`).

Usage: survive.py <apps-work> <index-rows.json> <humanify-ag> <out.json>"""
import json
import os
import re
import subprocess
import sys

WORK, ROWS, HA, OUT = sys.argv[1:5]
MIN = 20
B64 = {c: i for i, c in enumerate("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/")}
BUNDLERS = ("bun", "esbuild", "rollup", "webpack")


def vlq(seg):
    out, shift, val = [], 0, 0
    for c in seg:
        d = B64[c]
        val += (d & 31) << shift
        if d & 32:
            shift += 5
        else:
            out.append(-(val >> 1) if val & 1 else val >> 1)
            shift, val = 0, 0
    return out


def decode(mappings):
    """[(gen_line, gen_col16, src, orig_line, orig_col)] in generated order."""
    segs, src, ol, oc = [], 0, 0, 0
    for line, text in enumerate(mappings.split(";")):
        col = 0
        for seg in text.split(","):
            if not seg:
                continue
            v = vlq(seg)
            col += v[0]
            if len(v) >= 4:
                src += v[1]
                ol += v[2]
                oc += v[3]
                segs.append((line, col, src, ol, oc))
    return segs


class Bundle:
    def __init__(self, d):
        self.code = open(os.path.join(d, "out.js"), "rb").read()
        m = json.load(open(os.path.join(d, "out.js.map")))
        self.sources = [os.path.normpath(os.path.join(d, s)) for s in m["sources"]]
        segs = decode(m["mappings"])
        # byte offset of each segment: convert (line, utf16 col) -> byte offset
        text = self.code.decode("utf-8", "replace")
        lines = text.split("\n")
        starts, acc = [], 0
        for ln in lines:
            starts.append(acc)
            acc += len(ln.encode("utf-8")) + 1
        self.offsets = []
        cache = {}
        for gl, gc, s, ol, oc in segs:
            if gl >= len(lines):
                continue
            if gl not in cache:
                cache[gl] = utf16_to_byte_table(lines[gl])
            tbl = cache[gl]
            b = starts[gl] + (tbl[gc] if gc < len(tbl) else tbl[-1])
            self.offsets.append((b, s, ol, oc))
        self.offsets.sort()
        self.keys = [o[0] for o in self.offsets]

    def segments(self, start, end):
        import bisect

        i = bisect.bisect_left(self.keys, start)
        j = bisect.bisect_left(self.keys, end)
        return self.offsets[i:j]

    def owner(self, start, end):
        segs = self.segments(start, end)
        if not segs:
            return None, None
        tally = {}
        for _, s, _, _ in segs:
            tally[s] = tally.get(s, 0) + 1
        s, n = max(tally.items(), key=lambda kv: kv[1])
        if n < 0.9 * len(segs):
            return "MIXED", None
        first = next(x for x in segs if x[1] == s)
        return self.sources[s], (self.sources[s], first[2], first[3])

    def text(self, start, end):
        return self.code[start:end].decode("utf-8", "replace")


def utf16_to_byte_table(line):
    tbl, b = [], 0
    for ch in line:
        n16 = 2 if ord(ch) > 0xFFFF else 1
        nb = len(ch.encode("utf-8"))
        for _ in range(n16):
            tbl.append(b)
        b += nb
    tbl.append(b)
    return tbl


def package_of(path):
    """(package, file inside it) of a node_modules path, else (None, None)."""
    parts = path.split(os.sep)
    if "node_modules" not in parts:
        return None, None
    i = len(parts) - 1 - parts[::-1].index("node_modules")
    rest = parts[i + 1 :]
    if not rest:
        return None, None
    n = 2 if rest[0].startswith("@") else 1
    return "/".join(rest[:n]), "/".join(rest[n:])


def prints(d, bundler):
    out = os.path.join(d, "prints.json")
    if not os.path.exists(out):
        rc = subprocess.run([HA, "discover", os.path.join(d, "out.js"), "--unit-prints", "--bundler", bundler, "--out", out], capture_output=True).returncode
        if rc != 0:
            return None
    doc = json.load(open(out))
    rows = (doc.get("libraryDiscovery") or {}).get("unitRows", [])
    ps = []
    for r in rows:
        for p, whole in ([(r["whole"], True)] if "whole" in r else []) + [(f, False) for f in r.get("functions", [])]:
            if p["nodes"] >= MIN:
                ps.append({"hash": p["hash"], "nodes": p["nodes"], "start": p["start"], "end": p["end"], "whole": whole})
    return ps


HELPERS = {
    "esbuild": ("__toESM", "__commonJS", "__export", "__reExport", "__toCommonJS"),
    "rollup": ("getDefaultExportFromCjs", "commonjsGlobal", "getAugmentedNamespace", "||(", "=1,"),
    "webpack": ("__webpack_require__", ".d(", ".r(", ".n(", "__esModule"),
}
KEYWORDS = set(
    "var let const function return if else for while do switch case default break continue new typeof instanceof "
    "in of void delete throw try catch finally class extends super this null true false async await yield import "
    "export from static get set".split()
)
IDENT = re.compile(r"[A-Za-z_$][\w$]*")
NUM = re.compile(r"(?<![\w$])\d+(?:\.\d+)?(?:e\d+)?")


def blur(t):
    return IDENT.sub(lambda m: m.group(0) if m.group(0) in KEYWORDS else "x", t)


def spelling(t):
    """Operator and syntax spellings that differ between minifiers but not in meaning."""
    t = re.sub(r"\b(const|var)\b", "let", t)
    t = t.replace("!==", "!=").replace("===", "==")
    t = re.sub(r'("[^"]*"|\d+|void 0|!0|!1|null)(==|!=)(typeof [\w$.]+|[\w$.]+)', r"\3\2\1", t)  # yoda
    t = re.sub(r"\(([\w$]+)\)=>", r"\1=>", t)  # (n)=> vs n=>
    t = t.replace("void 0", "undefined").replace("!0", "true").replace("!1", "false")
    return t


def transform(bt, ot, bundler):
    """Why a print of `bundler` (text ot) differs from bun's print of the same source (bt):
    the first normalisation under which the two texts agree, else a structural guess."""
    if not bt:
        return "no-bun-counterpart"
    b0, o0 = blur(bt), blur(ot)
    if b0 == o0:
        return "same-text-modulo-names"
    if blur(spelling(bt)) == blur(spelling(ot)):
        return "operator-or-keyword-spelling"
    if any(h in ot and h not in bt for h in HELPERS.get(bundler, ())):
        return "module-wrapper-or-helper"
    if len(NUM.findall(ot)) != len(NUM.findall(bt)) or ot.count('"') != bt.count('"'):
        return "constant-inlining-or-folding"
    if bt.count("if(") != ot.count("if(") or bt.count("return") != ot.count("return"):
        return "if-return-to-expression"
    return "other-syntax"


def bun_counterpart(by_origin, p):
    """The bun print of the same source line and kind nearest in column (then node count)."""
    f, line, col, whole = p["origin"]
    cands = by_origin.get((f, line, whole), [])
    if not cands:
        return "", False
    best = min(cands, key=lambda c: (abs(c[0] - col), abs(c[3] - p["nodes"])))
    return best[1], best[2]


def main():
    apps = json.load(open(os.path.join(WORK, "apps.json")))
    index = json.load(open(ROWS))
    out = {}
    for app, rec in apps.items():
        pkg, ver = rec["package"], rec["version"]
        rows = index[pkg]["rows"]
        any_hashes = {r["hash"] for r in rows}
        ver_rows = {}
        for r in rows:
            if ver in r["versions"]:
                ver_rows.setdefault(r["hash"], r)
        ver_files = {r["file"] for r in ver_rows.values()}
        res = {"package": pkg, "version": ver, "indexedAtVersion": bool(ver_rows), "indexedRows": len(ver_rows), "indexedFiles": len(ver_files)}
        bun_by_origin = {}
        for b in BUNDLERS:
            d = os.path.join(WORK, "out", app, b)
            if not rec["builds"][b]["ok"]:
                res[b] = {"error": rec["builds"][b]["error"][:200]}
                continue
            bundle = Bundle(d)
            ps = prints(d, b)
            if ps is None:
                res[b] = {"error": "discover failed"}
                continue
            all_hashes = {p["hash"] for p in ps}
            surv, files, by_origin = [], set(), {}
            mixed = 0
            for p in ps:
                src, origin = bundle.owner(p["start"], p["end"])
                if src == "MIXED":
                    mixed += 1
                    continue
                if src is None:
                    continue
                owner, f = package_of(src)
                if owner != pkg:
                    continue
                p["file"], p["origin"] = f, (f, origin[1], origin[2], p["whole"])
                surv.append(p)
                files.add(f)
            # distinct prints by hash (a function print can repeat)
            seen = {}
            for p in surv:
                seen.setdefault(p["hash"], p)
            surv = list(seen.values())
            m_any = [p for p in surv if p["hash"] in any_hashes]
            m_ver = [p for p in surv if p["hash"] in ver_rows]
            if b == "bun":
                for p in surv:
                    f_, line, col, whole = p["origin"]
                    bun_by_origin.setdefault((f_, line, whole), []).append(
                        (col, bundle.text(p["start"], p["end"]), p["hash"] in any_hashes, p["nodes"])
                    )
            whys, samples = {}, {}
            for p in surv:
                if p["hash"] in any_hashes:
                    continue
                if b == "bun":
                    why = "bun-unmatched"
                    bt = ""
                else:
                    bt, bun_hit = bun_counterpart(bun_by_origin, p)
                    why = transform(bt, bundle.text(p["start"], p["end"]), b) if bun_hit else ("bun-also-unmatched" if bt else "no-bun-counterpart")
                whys[why] = whys.get(why, 0) + 1
                if len(samples.setdefault(why, [])) < 3:
                    samples[why].append({"file": p["file"], "nodes": p["nodes"], "this": bundle.text(p["start"], p["end"])[:300], "bun": bt[:300]})
            res[b] = {
                "bytes": len(bundle.code),
                "survivors": len(surv),
                "survivorNodes": sum(p["nodes"] for p in surv),
                "matchAny": len(m_any),
                "matchAnyNodes": sum(p["nodes"] for p in m_any),
                "matchVersion": len(m_ver),
                "survivingFiles": len(files & ver_files),
                "survivingFilesAll": len(files),
                "survivedRows": sum(1 for h in ver_rows if h in all_hashes),
                "mixedPrints": mixed,
                "unmatched": whys,
                "samples": samples,
            }
        out[app] = res
        print(app, pkg, ver, {b: (res[b].get("survivors"), res[b].get("matchAny")) for b in BUNDLERS}, flush=True)
    json.dump(out, open(OUT, "w"), indent=1)
    table(out)


def table(out):
    """Per-bundler totals and per-app survival (markdown)."""
    print("| bundler | apps | survivors | matched (any version) | by nodes | matched at the installed version | indexed rows at version (sum) | survivor/indexed | files surviving / indexed |")
    print("|---|---|---|---|---|---|---|---|---|")
    for b in BUNDLERS:
        rs = [r for r in out.values() if "survivors" in r.get(b, {})]
        t = lambda k: sum(r[b][k] for r in rs)  # noqa: E731
        idx = sum(r["indexedRows"] for r in rs)
        files = sum(r["indexedFiles"] for r in rs)
        print(
            f"| {b} | {len(rs)} | {t('survivors')} | {t('matchAny')} ({100 * t('matchAny') / max(1, t('survivors')):.1f}%) "
            f"| {100 * t('matchAnyNodes') / max(1, t('survivorNodes')):.1f}% | {t('matchVersion')} | {idx} "
            f"| {100 * t('survivors') / max(1, idx):.1f}% | {t('survivingFiles')} / {files} |"
        )
    why = {}
    for r in out.values():
        for b in BUNDLERS:
            for k, v in r.get(b, {}).get("unmatched", {}).items():
                why.setdefault(b, {}).setdefault(k, 0)
                why[b][k] += v
    for b, v in why.items():
        print(b, "unmatched:", json.dumps(dict(sorted(v.items(), key=lambda kv: -kv[1]))))
    print("\n| app | package@version | indexed rows | " + " | ".join(f"{b} surv/match" for b in BUNDLERS) + " |")
    print("|---|---|---|" + "---|" * len(BUNDLERS))
    for app, r in out.items():
        cells = [f"{r[b]['survivors']}/{r[b]['matchAny']}" if "survivors" in r.get(b, {}) else "fail" for b in BUNDLERS]
        print(f"| {app} | {r['package']}@{r['version']} | {r['indexedRows']} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
