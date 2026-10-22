"""exp136: does another BUILD SHAPE of the truth's own version carry a wrong seed's unit? Sizing without the
builder: for every census row (noprint.py --detail rows / side rows) whose truth version is installed in the index's
work dir, build the truth package's app entry again in a scratch dir under each variant, fingerprint the output
with `humanify discover --unit-prints`, and ask whether the unit's prints are now there.

Variants (the index builds `req` = require() of every subpath, `dyn` = import() of every subpath):
  req, dyn       the index's own builds of that slice, re-fingerprinted (the method's control: a d row must read 0)
  sta-cjs        a STATIC ESM entry (`import * as mN from "<subpath>"`), the slice's format (cjs)
  sta-esm        the same entry, --format esm (an app bundle written as ESM)
Recovered = the unit's WHOLE print is in the build (class f: the truth already holds function prints, only the
whole closes the lookup); `anyprint` = any of its prints (whole or a function print of >= 30 nodes).
Bytes are attributed to the first variant (in the order above) that recovers them, and per variant on its own.

usage: python3 -I variants.py <out.json> <rows.jsonl>... [--classes d-print-differs,f-filtered,...] [--jobs N]
prints KB per (set, class/sub) x variant; package names are never printed (rows of held-out come from the
census's scratch side file)."""
import collections
import concurrent.futures as cf
import json
import os
import re
import subprocess
import sys

WORK = "/work/exp135-data/full/work"
SCRATCH = os.environ.get("VARIANTS_SCRATCH", "/work/exp136-data/variants")
BIN = os.environ.get("HUMANIFY", "/Users/andrewgross/Development/humanify/target/release/humanify")
BUN = "/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun"
ESBUILD = "/work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild"
SCRIPTS = {"rollup": "/work/exp105-data/tools/rollup-build.mjs", "webpack": "/work/exp105-data/tools/webpack-build.cjs"}
NODE_ENV = 'process.env.NODE_ENV="production"'
VARIANTS = ("req", "dyn", "sta-cjs", "sta-esm")


def esc(p):
    return p.replace("/", "__")


def entry_text(work_dir):
    """The static ESM entry over the subpaths the index's req entry requires."""
    req = open(os.path.join(work_dir, "entries", "req.cjs")).read()
    specs = re.findall(r'require\((".*?")\);', req)
    lines = ['function labAppMain_sta(x){ if (!x) throw new Error("lab-app sta: no input given"); '
             'return Object.keys(x).length + ":sta"; }']
    lines += [f"import * as m{i} from {s};" for i, s in enumerate(specs)]
    lines.append("export { labAppMain_sta, " + ", ".join(f"m{i}" for i in range(len(specs))) + " };")
    return "\n".join(lines) + "\n"


def command(sl, entry, out, fmt):
    bundler, target = sl.split("-")[:2]
    if bundler == "bun":
        return [BUN, "build", entry, "--target", target, "--format", fmt, "--minify", "--sourcemap=external",
                "--outdir", out, "--define", NODE_ENV]
    if bundler == "esbuild":
        return [ESBUILD, entry, "--bundle", "--platform=node", "--log-level=error", f"--format={fmt}", "--minify",
                "--sourcemap=external", "--sources-content=false", f"--outfile={out}/out.js"]
    return ["node", SCRIPTS[bundler], entry, out, fmt]


def prints(js):
    o = js + ".prints.json"
    if not os.path.exists(o):
        r = subprocess.run([BIN, "discover", js, "--unit-prints", "--out", o], capture_output=True, timeout=900)
        if r.returncode or not os.path.exists(o):
            return None
    d = json.load(open(o)).get("libraryDiscovery") or {}
    whole, anyp = set(), set()
    for u in d.get("unitRows", ()):
        if "whole" in u:
            whole.add(u["whole"]["hash"])
            anyp.add(u["whole"]["hash"])
        anyp.update(f["hash"] for f in u.get("functions", ()) if f["nodes"] >= 30)
    return whole, anyp


def build(pv, sl):
    """{variant: (whole hashes, any hashes) | None (build failed / not installed)}."""
    p, v = pv.rsplit("@", 1)
    w = os.path.join(WORK, f"{esc(p)}@{v}")
    out = {k: None for k in VARIANTS}
    if not os.path.exists(os.path.join(w, "entries", "req.cjs")):
        return pv, sl, out
    for tag in ("req", "dyn"):
        js = os.path.join(w, "build", sl, tag, "out.js")
        if os.path.exists(js):
            dst = os.path.join(SCRATCH, esc(pv), sl, tag)
            os.makedirs(dst, exist_ok=True)
            if not os.path.exists(dst + "/out.js"):
                subprocess.run(["cp", js, dst + "/out.js"])
            out[tag] = prints(dst + "/out.js")
    d = os.path.join(SCRATCH, esc(pv), "app")
    os.makedirs(d + "/entries", exist_ok=True)
    if not os.path.lexists(d + "/node_modules"):
        os.symlink(os.path.join(w, "node_modules"), d + "/node_modules")
    open(d + "/entries/sta.mjs", "w").write(entry_text(w))
    for fmt in ("cjs", "esm"):
        o = os.path.join(SCRATCH, esc(pv), sl, "sta-" + fmt)
        js = o + "/out.js"
        if not os.path.exists(js):
            os.makedirs(o, exist_ok=True)
            try:
                r = subprocess.run(command(sl, "entries/sta.mjs", o, fmt), cwd=d, capture_output=True, timeout=900)
            except subprocess.TimeoutExpired:
                continue
            if r.returncode:
                continue
            if sl.startswith("bun") and os.path.exists(o + "/sta.js"):
                os.rename(o + "/sta.js", js)
        if os.path.exists(js):
            out["sta-" + fmt] = prints(js)
    return pv, sl, out


def main():
    dest = sys.argv[1]
    files = [a for a in sys.argv[2:] if a.endswith(".jsonl")]
    classes = (sys.argv[sys.argv.index("--classes") + 1].split(",") if "--classes" in sys.argv
               else ["d-print-differs", "f-filtered"])
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 12
    rows = []
    for f in files:
        for line in open(f):
            r = json.loads(line)
            if r["cls"] in classes and r.get("truth") and r.get("version") and r.get("truthVersionIndexed"):
                rows.append(r)
    todo = sorted({(f"{r['truth']}@{r['version']}", r["slice"]) for r in rows})
    print(f"# {len(rows)} rows, {len(todo)} (version, slice) builds", file=sys.stderr)
    res = {}
    with cf.ThreadPoolExecutor(jobs) as ex:
        for pv, sl, out in ex.map(lambda t: build(*t), todo):
            res[(pv, sl)] = out
    tot = collections.Counter()
    by = collections.Counter()
    first = collections.Counter()
    for r in rows:
        k = (r["set"], r["cls"], r["sub"].split("/")[0] if r["cls"] == "f-filtered" else r["sub"].split("-same-size")[0],
             "esm-out" if r.get("esmOut") else "cjs-out")
        tot[k] += r["bytes"]
        out = res[(f"{r['truth']}@{r['version']}", r["slice"])]
        hs = set(r["hashes"])
        whole = r.get("whole")
        got = None
        for var in VARIANTS:
            pr = out[var]
            if pr is None:
                by[k + (var, "failed")] += r["bytes"]
                continue
            if whole in pr[0]:
                by[k + (var, "whole")] += r["bytes"]
                got = got or var
            elif hs & pr[1]:
                by[k + (var, "anyprint")] += r["bytes"]
        first[k + (got or "none",)] += r["bytes"]
    table = []
    for k in sorted(tot):
        rec = {"set": k[0], "cls": k[1], "sub": k[2], "format": k[3], "kb": round(tot[k] / 1e3, 1)}
        for var in VARIANTS:
            rec[var] = {m: round(by[k + (var, m)] / 1e3, 1) for m in ("whole", "anyprint", "failed")}
        rec["firstWhole"] = {var: round(first[k + (var,)] / 1e3, 1) for var in VARIANTS + ("none",)}
        table.append(rec)
        print(f"{k[0]:14} {k[1]:16} {k[2]:22} {k[3]:8} {tot[k] / 1e3:8.1f} KB  "
              + "  ".join(f"{var} {by[k + (var, 'whole')] / 1e3:.1f}/{by[k + (var, 'anyprint')] / 1e3:.1f}"
                          f"(x{by[k + (var, 'failed')] / 1e3:.1f})" for var in VARIANTS))
    json.dump(table, open(dest, "w"), indent=1)


if __name__ == "__main__":
    main()
