"""exp105: the DEV-SIDE webpack and rollup builds — small apps made here, from packages of the step-4 index's world
(never from exp100's test split), built with the index builder's own webpack and rollup recipes, then frozen as the
exp096 instrument's third corpus, `bundlers` (corpus-bundlers.json; truth and origin layer exactly as exp096 builds
them for the held-out apps).

usage: <exp095 venv python> -I dev_apps.py build [--jobs N]    -> $DATA/apps/<id>/, $DATA/build/<bundler>/<id>/
       <exp095 venv python> -I dev_apps.py freeze [--jobs N]   -> $DATA/frozen/, corpus-bundlers.json,
                                                                  origins-bundlers.json

Each app (seeded, SEED below): ROOTS packages drawn from the step-4 base list (huge, native and type-only packages
left out) plus one popular package that is NOT a step-4 root (it may still be indexed as a dependency), installed
as of the index's date (npm --before), and ~40 lines of app code that imports each and does its own work (the app
bytes the app-claimed bar reads). A bundler build that fails leaves that app out of that set only (recorded).
"""
import json
import os
import random
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "../../.."))
sys.path.insert(0, os.path.join(REPO, "experiments/096-library-matching-score"))
import lib096  # noqa: E402

DATA = "/work/exp105-data/dev"
TOOLS = "/work/exp105-data/tools"  # webpack + rollup + plugins, and copies of crates/humanify-index/recipes/
BASE = "/work/exp097-data/base-step4.txt"
AS_OF = "2026-10-11"
SEED = 105
APPS = 14
ROOTS = 6
EXTRAS = ["citty", "defu", "dequal", "got", "hookable", "klona", "kolorist", "ky", "listr2", "nanospinner", "ofetch",
          "pathe", "pino", "sade", "scule"]
SKIP_PREFIX = ("@types/", "@esbuild/", "@swc/", "@napi-rs/", "@img/", "@rollup/rollup-", "@next/swc", "electron",
               "typescript", "esbuild", "prettier", "sharp", "puppeteer", "playwright", "node-gyp", "@biomejs/",
               "@typescript-eslint/", "eslint", "webpack", "rollup", "vite", "@babel/", "terser", "uglify",
               "@tootallnate/quickjs", "monaco", "pdfjs", "@aws-sdk/client-", "aws-sdk", "googleapis", "@azure/arm-",
               "fsevents", "canvas", "sqlite3", "better-sqlite3", "@prisma/", "prisma", "lightningcss", "bun")
BUNDLERS = {"webpack": "webpack-build.cjs", "rollup": "rollup-build.mjs"}
DESC = {
    "webpack": "webpack 5.111.1 production (terser), target node, one chunk, source-map (humanify-index's recipe)",
    "rollup": "rollup 4.64.5 + node-resolve + commonjs + json + terser, format cjs, sourcemap (humanify-index's "
              "recipe)",
}

APP_CODE = '''
const DEV105_DEFAULTS = {{ retries: {retries}, label: "dev105-{n}", verbose: false }};
function dev105ParseArgs(argv) {{
  const out = {{ ...DEV105_DEFAULTS, rest: [] }};
  for (let i = 0; i < argv.length; i++) {{
    const a = argv[i];
    if (a === "--verbose") out.verbose = true;
    else if (a.startsWith("--retries=")) out.retries = Number(a.slice(10)) || DEV105_DEFAULTS.retries;
    else if (a.startsWith("--label=")) out.label = a.slice(8);
    else out.rest.push(a);
  }}
  return out;
}}
function dev105Describe(mod, name) {{
  const keys = mod && typeof mod === "object" ? Object.keys(mod) : [];
  const kind = typeof mod === "function" ? "function" : keys.length ? "namespace(" + keys.length + ")" : typeof mod;
  return name.padEnd({pad}, ".") + " " + kind;
}}
class Dev105Report {{
  constructor(label) {{ this.label = label; this.lines = []; this.started = Date.now(); }}
  add(line) {{ this.lines.push("[" + this.label + "] " + line); return this; }}
  render() {{ return this.lines.join("\\n") + "\\n(" + this.lines.length + " modules, " + (Date.now() - this.started) + " ms)"; }}
}}
export function dev105Main(argv) {{
  const opts = dev105ParseArgs(argv);
  const report = new Dev105Report(opts.label);
  for (const [name, mod] of DEV105_MODULES) report.add(dev105Describe(mod, name));
  if (opts.verbose) console.error("dev105: " + opts.rest.length + " extra argument(s)");
  return report.render();
}}
console.log(dev105Main(process.argv.slice(2)));
'''


def run(cmd, cwd, timeout=1800):
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        return p.returncode, (p.stderr + p.stdout)[-3000:]
    except subprocess.TimeoutExpired:
        return 124, "timeout"


def candidates():
    names = [n.strip() for n in open(BASE) if n.strip()]
    return [n for n in names if not n.startswith(SKIP_PREFIX)]


def plan():
    rng = random.Random(SEED)
    pool = candidates()
    apps = []
    for i in range(APPS):
        roots = rng.sample(pool, ROOTS)
        extra = rng.choice(EXTRAS)
        apps.append({"id": f"dev105-app{i:02d}", "roots": roots, "extra": extra,
                     "retries": rng.randint(1, 9), "pad": rng.randint(18, 40)})
    return apps


def entry_code(app, deps):
    imports = "".join(f'import * as m{i} from {json.dumps(d)};\n' for i, d in enumerate(deps))
    table = ", ".join(f'[{json.dumps(d)}, m{i}]' for i, d in enumerate(deps))
    return (imports + f"const DEV105_MODULES = [{table}];\n" +
            APP_CODE.format(n=app["id"][-2:], retries=app["retries"], pad=app["pad"]))


def install(app):
    d = os.path.join(DATA, "apps", app["id"])
    if os.path.exists(os.path.join(d, ".installed")):
        return d, json.load(open(os.path.join(d, "deps.json")))
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    deps = []
    for name in app["roots"] + [app["extra"]]:
        # one at a time: a package that will not install (or resolve) is left out, never the whole app
        rc, log = run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error",
                       "--omit=dev", "--legacy-peer-deps", f"--before={AS_OF}", name], d, timeout=900)
        if rc == 0:
            deps.append(name)
    json.dump(deps, open(os.path.join(d, "deps.json"), "w"))
    open(os.path.join(d, ".installed"), "w").write("")
    return d, deps


def build(app):
    d, deps = install(app)
    res = {"id": app["id"], "deps": deps}
    for bundler, script in BUNDLERS.items():
        out = os.path.join(DATA, "build", bundler, app["id"])
        if os.path.exists(os.path.join(out, "out.js.map")):
            res[bundler] = True
            continue
        kept = list(deps)
        ok = False
        for _ in range(4):
            open(os.path.join(d, "entry.mjs"), "w").write(entry_code(app, kept))
            shutil.rmtree(out, ignore_errors=True)
            os.makedirs(out)
            rc, log = run(["node", "--max-old-space-size=8192", os.path.join(TOOLS, script), "entry.mjs", out, "cjs"],
                          d)
            if rc == 0 and os.path.exists(os.path.join(out, "out.js.map")):
                ok = True
                break
            # drop the package the error names, if any, and retry
            bad = [k for k in kept if f'"{k}"' in log or f"'{k}'" in log or f"/{k}/" in log]
            if not bad:
                res[bundler + "Error"] = log[-600:]
                break
            kept = [k for k in kept if k not in bad]
        res[bundler] = ok
        if ok:
            res[bundler + "Deps"] = kept
            open(os.path.join(out, "entry.mjs"), "w").write(entry_code(app, kept))
    return res


def do_build(jobs):
    apps = plan()
    os.makedirs(DATA, exist_ok=True)
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(build, apps))
    json.dump({"seed": SEED, "asOf": AS_OF, "apps": apps, "results": results},
              open(os.path.join(DATA, "apps.json"), "w"), indent=1)
    for r in results:
        print(r["id"], "webpack", r.get("webpack"), "rollup", r.get("rollup"), len(r["deps"]), "deps")


def do_freeze(jobs):
    import multiprocessing
    lib096.use("bundlers")
    import corpus
    built = json.load(open(os.path.join(DATA, "apps.json")))
    manifest = {"schema": "exp096-corpus/1", "corpus": "bundlers", "frozen": True,
                "note": "exp105 DEV-SIDE webpack and rollup builds of small apps made from the step-4 index's "
                        "packages (experiments/105-webpack-rollup/scripts/dev_apps.py): a dev corpus, never the "
                        "test split", "entries": []}
    work = []
    for r in built["results"]:
        for bundler in BUNDLERS:
            if not r.get(bundler):
                continue
            src_dir = os.path.join(DATA, "build", bundler, r["id"])
            entry = {"id": f"{bundler}-{r['id']}", "set": f"dev-{bundler}", "bundler": bundler, "builtWith": bundler,
                     "appPackage": "-", "kind": "dev105",
                     "description": f"exp105 dev app {r['id']} ({', '.join(r[bundler + 'Deps'])}): {DESC[bundler]}"}
            d = os.path.join(lib096.DATA, "corpus", entry["id"])
            os.makedirs(d, exist_ok=True)
            entry["files"] = {}
            for role, name in (("bundle", "out.js"), ("map", "out.js.map")):
                src, dst = os.path.join(src_dir, name), os.path.join(d, name)
                shutil.copyfile(src, dst)
                entry["files"][role] = {"name": name, "sha256": lib096.sha256_file(dst),
                                        "bytes": os.path.getsize(dst), "origin": src}
            entry["truth"] = {"method": "sourcemap"}
            manifest["entries"].append(entry)
            # the map's sources are relative to the build dir: versions come from the app's install
            work.append((entry, {"bundle": os.path.join(src_dir, "out.js")},
                         ("install", src_dir, None)))
    with multiprocessing.Pool(jobs) as pool:
        shas = dict(pool.map(corpus.build_truth, work, chunksize=1))
    for e in manifest["entries"]:
        e["truth"]["sha256"] = shas[e["id"]]
    open(lib096.MANIFEST, "w").write(lib096.dump(manifest))
    print(f"froze {len(manifest['entries'])} entries -> {lib096.MANIFEST}")
    corpus.freeze_origins(jobs)


if __name__ == "__main__":
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 6
    {"build": do_build, "freeze": do_freeze}[sys.argv[1]](jobs)
