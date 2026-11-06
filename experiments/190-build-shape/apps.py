#!/usr/bin/env python3
"""exp190 (c): generate the tiny apps and bundle each with bun, esbuild,
rollup and webpack (production, minified, CommonJS output, sourcemap).

Each app imports 1-3 named exports (or the default export of a CommonJS
package) from ONE popular package, at the newest version the cb64 index
holds, and exports them so no bundler can drop them. One shared npm project
holds every package (--ignore-scripts; react is installed for lucide-react
and kept external like a real React app's peer).

Bundler settings:
  bun      bun build --target bun --format cjs --minify --define NODE_ENV
           (the index's own bun-bun-cjs-min recipe, so its leg is the
           reference: what the index would see for a tree-shaken import)
  esbuild  --bundle --platform=node --format=cjs --minify --define NODE_ENV
  rollup   the index builder's rollup recipe script (rollup 4 +
           node-resolve + commonjs + json + terser), cjs
  webpack  the index builder's webpack recipe script (webpack 5 production,
           terser), commonjs2
The rollup/webpack scripts and tools come from an existing tool store
(exp181's). Usage: apps.py <work> <index-dir> <bun> <esbuild> <rollup-script> <webpack-script>"""
import json
import os
import shutil
import subprocess
import sys
import time

WORK, INDEX, BUN, ESBUILD, ROLLUP, WEBPACK = sys.argv[1:7]
DEFINE = 'process.env.NODE_ENV="production"'

# (app id, package, import clause) — `{a, b}` named, or `default` for a default import
APPS = [
    ("lodash-es-1", "lodash-es", "{ debounce }"),
    ("lodash-es-3", "lodash-es", "{ debounce, throttle, cloneDeep }"),
    ("date-fns-1", "date-fns", "{ format }"),
    ("date-fns-3", "date-fns", "{ addDays, differenceInDays, parseISO }"),
    ("zod-1", "zod", "{ z }"),
    ("zod-2", "zod", "{ string, object }"),
    ("rxjs-2", "rxjs", "{ of, map }"),
    ("rxjs-3", "rxjs", "{ Subject, debounceTime, distinctUntilChanged }"),
    ("lucide-react-1", "lucide-react", "{ Camera }"),
    ("lucide-react-3", "lucide-react", "{ ArrowRight, Check, X }"),
    ("query-core-1", "@tanstack/query-core", "{ QueryClient }"),
    ("table-core-2", "@tanstack/table-core", "{ createTable, getCoreRowModel }"),
    ("uuid-1", "uuid", "{ v4 }"),
    ("uuid-3", "uuid", "{ v4, v7, validate }"),
    ("nanoid-1", "nanoid", "{ nanoid }"),
    ("ramda-2", "ramda", "{ map, pipe }"),
    ("immer-1", "immer", "{ produce }"),
    ("zustand-1", "zustand", "{ create }"),
    ("yaml-1", "yaml", "{ parse }"),
    ("semver-1", "semver", "{ satisfies }"),
    ("lodash-1", "lodash", "{ debounce }"),
    ("dayjs-d", "dayjs", "default"),
    ("clsx-1", "clsx", "{ clsx }"),
    ("fast-deep-equal-d", "fast-deep-equal", "default"),
    ("deepmerge-d", "deepmerge", "default"),
    ("ms-d", "ms", "default"),
    ("change-case-2", "change-case", "{ camelCase, snakeCase }"),
    ("valibot-3", "valibot", "{ object, string, parse }"),
    ("es-toolkit-2", "es-toolkit", "{ debounce, chunk }"),
    ("remeda-3", "remeda", "{ pipe, map, filter }"),
    ("d3-array-2", "d3-array", "{ extent, sum }"),
    ("marked-1", "marked", "{ marked }"),
    ("redux-toolkit-2", "@reduxjs/toolkit", "{ createSlice, configureStore }"),
    ("superjson-1", "superjson", "{ stringify }"),
]
EXTERNAL = ["react", "react-dom", "react/jsx-runtime"]
BUNDLERS = ("bun", "esbuild", "rollup", "webpack")


def run(cmd, cwd=None):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=1800)
    return p.returncode, (p.stdout + p.stderr)[-2000:]


def strip_maps(node_modules):
    for d, _dirs, files in os.walk(node_modules):
        for f in files:
            if not f.endswith((".js", ".mjs", ".cjs")):
                continue
            p = os.path.join(d, f)
            try:
                text = open(p, encoding="utf-8").read()
            except (UnicodeDecodeError, OSError):
                continue
            if "sourceMappingURL=" in text:
                kept = [ln for ln in text.split("\n") if not ln.startswith(("//# sourceMappingURL=", "//@ sourceMappingURL="))]
                open(p, "w", encoding="utf-8").write("\n".join(kept))


def newest_indexed():
    versions = json.load(open(os.path.join(INDEX, "versions.json")))
    return {p: versions[p][-1] for _, p, _ in APPS}


def entry_text(pkg, clause):
    if clause == "default":
        names = ["lib"]
        imp = f'import lib from "{pkg}";'
    else:
        names = [n.strip() for n in clause.strip("{} ").split(",")]
        imp = f'import {clause} from "{pkg}";'
    return f"{imp}\nexport const used = [{', '.join(names)}];\nexport function appMain(x) {{ return used.length + x; }}\n"


def bundle(bundler, entry, out):
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    if bundler == "bun":
        cmd = [BUN, "build", entry, "--target", "bun", "--format", "cjs", "--minify", "--sourcemap=external"]
        cmd += ["--outdir", out, "--define", DEFINE] + [a for x in EXTERNAL for a in ("--external", x)]
    elif bundler == "esbuild":
        cmd = [ESBUILD, entry, "--bundle", "--platform=node", "--format=cjs", "--minify", "--log-level=error"]
        cmd += ["--sourcemap=external", "--sources-content=false", f"--outfile={out}/out.js", f"--define:{DEFINE}"]
        cmd += [f"--external:{x}" for x in EXTERNAL]
    else:
        cmd = ["node", ROLLUP if bundler == "rollup" else WEBPACK, entry, out, "cjs"] + EXTERNAL
    t0 = time.time()
    rc, msg = run(cmd, cwd=WORK)
    if rc == 0 and bundler == "bun":
        stem = os.path.splitext(os.path.basename(entry))[0]
        os.rename(os.path.join(out, stem + ".js"), os.path.join(out, "out.js"))
        os.rename(os.path.join(out, stem + ".js.map"), os.path.join(out, "out.js.map"))
    return {"ok": rc == 0, "seconds": time.time() - t0, "error": None if rc == 0 else msg[-600:]}


def main():
    os.makedirs(WORK, exist_ok=True)
    pins = newest_indexed()
    if not os.path.exists(os.path.join(WORK, ".installed")):
        deps = dict(pins)
        deps["react"] = "19.2.0"
        json.dump({"private": True, "name": "exp190-apps", "dependencies": deps}, open(os.path.join(WORK, "package.json"), "w"), indent=1)
        rc, msg = run(["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"], cwd=WORK)
        if rc != 0:
            sys.exit("npm install: " + msg)
        open(os.path.join(WORK, ".installed"), "w").write("")
    # as the index builder does: no bundler chains a package's own input maps
    # (a chained map attributes code to the package's .ts sources)
    strip_maps(os.path.join(WORK, "node_modules"))
    os.makedirs(os.path.join(WORK, "apps"), exist_ok=True)
    report = {}
    for app, pkg, clause in APPS:
        entry = os.path.join("apps", f"{app}.mjs")
        open(os.path.join(WORK, entry), "w").write(entry_text(pkg, clause))
        installed = json.load(open(os.path.join(WORK, "node_modules", pkg, "package.json")))["version"]
        rec = {"package": pkg, "clause": clause, "version": installed, "indexedPin": pins[pkg], "builds": {}}
        for b in BUNDLERS:
            rec["builds"][b] = bundle(b, entry, os.path.join(WORK, "out", app, b))
        report[app] = rec
        print(app, {b: r["ok"] for b, r in rec["builds"].items()}, flush=True)
    json.dump(report, open(os.path.join(WORK, "apps.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
