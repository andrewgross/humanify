#!/usr/bin/env python3
"""exp190 (b): the two build routes for one package@version, timed.

BUNDLE route — a reproduction of what `humanify-ag index build` does for the
`bun-bun-cjs-min` slice (crates/humanify-ag-index/src/build.rs): npm install
as of the version's publish time (--ignore-scripts), sourcemap comments
stripped, the two app entries (require() + import() of every Node export
subpath), `bun build --target bun --format cjs --minify` with the production
define, unresolvable specifiers retried as externals; then the unit prints of
each bundle through `humanify-ag discover --unit-prints --bundler bun`.

DIRECT routes — no install, no module graph: the package's own tarball
(`npm pack`), every .js/.mjs/.cjs file (not a type declaration, at most
4 MB; sponge-named files kept) minified on its own by the same Bun with the
slice's minify + define, in one of three ways (MODES below), then the unit
prints of each minified file through the same discover verb.

Usage: routes.py <picks.tsv> <out-root> <humanify-ag> <bun> <registry-dir> [routes]
routes: comma list of bundle, nobundle, single, single-esm, pkg-bundle (default all).
Writes <out-root>/<pkg>@<ver>/{bundle,direct-<route>}/... with
timing.json (the bundle route) and timing-<route>.json (each direct route).
No pipeline code runs except the discover verb; nothing is executed from
the installed packages (installs and packs use --ignore-scripts)."""
import json
import os
import re
import shutil
import subprocess
import sys
import time

PICKS, ROOT, HA, BUN, REGISTRY = sys.argv[1:6]
ROUTES = (sys.argv[6] if len(sys.argv) > 6 else "bundle,nobundle,single,single-esm,pkg-bundle").split(",")
DEFINE = 'process.env.NODE_ENV="production"'


def run(cmd, cwd=None, timeout=1800):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout + p.stderr


def safe(name):
    return name.replace("/", "__")


def is_js(name):
    base = name.rsplit("/", 1)[-1]
    return base.endswith((".js", ".mjs", ".cjs")) and ".d." not in base


def walk(root):
    out = []
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x != "node_modules"]
        for f in files:
            out.append(os.path.relpath(os.path.join(d, f), root).replace(os.sep, "/"))
    return sorted(out)


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
            if "sourceMappingURL=" not in text:
                continue
            kept = [
                ln
                for ln in text.split("\n")
                if not (ln.startswith("//# sourceMappingURL=") or ln.startswith("//@ sourceMappingURL="))
            ]
            open(p, "w", encoding="utf-8").write("\n".join(kept))


# --- entry_specifiers (a port of recipe.rs) ---
def node_targets(v):
    if isinstance(v, str):
        return [] if ".d." in v else [v]
    if isinstance(v, list):
        for x in v:
            t = node_targets(x)
            if t:
                return t[:1]
        return []
    if isinstance(v, dict):
        out = []
        for c in ("node", "require", "import", "default"):
            if c in v:
                out += node_targets(v[c])
        return out
    return []


def expand_wildcard(key, target, files):
    target = target[2:] if target.startswith("./") else target
    if "*" not in target or "*" not in key:
        return []
    t_pre, t_suf = target.split("*", 1)
    k_pre, k_suf = key.split("*", 1)
    out = []
    for f in files:
        n = f.rsplit("/", 1)[-1]
        if not (n.endswith((".js", ".cjs", ".mjs")) and ".d." not in n):
            continue
        if f.startswith(t_pre) and f.endswith(t_suf) and len(f) > len(t_pre) + len(t_suf):
            rest = f[len(t_pre) : len(f) - len(t_suf)]
            out.append(f"{k_pre}{rest}{k_suf}")
    return out


def entry_specifiers(name, manifest, files):
    exports = manifest.get("exports")
    if exports is None:
        return [name]
    if isinstance(exports, dict) and any(k.startswith(".") for k in exports):
        subpaths = [(k, v) for k, v in exports.items() if k.startswith(".")]
    else:
        subpaths = [(".", exports)]
    shipped = set(files)
    out = []
    for key, target in subpaths:
        targets = node_targets(target)
        if key.endswith(".json") or key.endswith("/") or not targets:
            continue
        if "*" in key:
            out += [f"{name}{s[1:]}" for s in expand_wildcard(key, targets[0], files)]
        elif all((t[2:] if t.startswith("./") else t) in shipped for t in targets):
            out.append(name if key == "." else f"{name}{key[1:]}")
    out = sorted(set(out))[:256]
    return out or [name]


def unresolved(err):
    out = []
    for marker, q in (('Could not resolve: "', '"'), ('Could not resolve "', '"'), ("Could not resolve: '", "'")):
        for part in err.split(marker)[1:]:
            spec = part.split(q, 1)[0]
            if spec and spec not in out:
                out.append(spec)
    return out


def bun_bundle(cwd, entry, outdir, start_externals=()):
    externals, esm, last = list(start_externals), False, ""
    for _ in range(6):
        shutil.rmtree(outdir, ignore_errors=True)
        os.makedirs(outdir)
        cmd = [BUN, "build", entry, "--target", "bun", "--format", "esm" if esm else "cjs", "--minify"]
        cmd += ["--sourcemap=external", "--outdir", outdir, "--define", DEFINE]
        for x in externals:
            cmd += ["--external", x]
        rc, last = run(cmd, cwd=cwd)
        if rc == 0:
            stem = os.path.splitext(os.path.basename(entry))[0]
            os.rename(os.path.join(outdir, stem + ".js"), os.path.join(outdir, "out.js"))
            return True, externals
        new = [s for s in unresolved(last) if s not in externals]
        if new and len(externals) < 64 + len(start_externals):
            externals += new
        elif "top-level await" in last.lower() and not esm:
            esm = True
        else:
            break
    return False, last.splitlines()[:1]


def discover(path, out):
    rc, msg = run([HA, "discover", path, "--unit-prints", "--bundler", "bun", "--out", out])
    return rc == 0


def write_entries(d, name):
    """The builder's two app entries (build.rs write_entries)."""
    root = os.path.join(d, "node_modules", name)
    manifest = json.load(open(os.path.join(root, "package.json")))
    specs = entry_specifiers(name, manifest, walk(root))
    q = [json.dumps(s) for s in specs]
    os.makedirs(os.path.join(d, "entries"), exist_ok=True)
    app = lambda tag: (  # noqa: E731
        f'function labAppMain_{tag}(x){{ if (!x) throw new Error("lab-app {tag}: no input given"); '
        f'return Object.keys(x).length + ":{tag}"; }}\n'
    )
    req = app("req") + "".join(f"const m{i} = require({s});\n" for i, s in enumerate(q))
    req += "module.exports = { run: labAppMain_req, " + ", ".join(f"m{i}" for i in range(len(q))) + " };\n"
    dyn = app("dyn") + "export const loaders = [" + ", ".join(f"() => import({s})" for s in q) + "];\n"
    dyn += "export { labAppMain_dyn };\n"
    open(os.path.join(d, "entries", "req.cjs"), "w").write(req)
    open(os.path.join(d, "entries", "dyn.mjs"), "w").write(dyn)
    return specs, manifest


def build_and_print(d, name, externals):
    """Write the entries, bundle both, print both: {build, fingerprint, builds, specs}."""
    specs, _ = write_entries(d, name)
    t = {}
    t0 = time.time()
    builds = {}
    for tag, entry in (("req", "entries/req.cjs"), ("dyn", "entries/dyn.mjs")):
        ok, info = bun_bundle(d, entry, os.path.join(d, "build", tag), externals)
        builds[tag] = {"ok": ok, "info": info}
    t["build"] = time.time() - t0
    t0 = time.time()
    for tag in builds:
        if builds[tag]["ok"]:
            builds[tag]["prints"] = discover(os.path.join(d, "build", tag, "out.js"), os.path.join(d, f"prints-{tag}.json"))
    t["fingerprint"] = time.time() - t0
    t["builds"], t["specs"] = builds, specs
    return t


def pkg_bundle_route(name, version, base):
    """The builder's two entries over the package's OWN tarball, every
    dependency external: the package's internal module graph is bundled as
    the builder bundles it, no dependency is installed."""
    d = os.path.join(base, "direct-pkg-bundle")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(os.path.join(d, "node_modules"))
    t0 = time.time()
    rc, msg = run(["npm", "pack", f"{name}@{version}", "--silent", "--ignore-scripts"], cwd=d)
    if rc != 0:
        return {"error": "pack: " + msg[:300]}
    tgz = [f for f in os.listdir(d) if f.endswith(".tgz")][0]
    run(["tar", "xzf", tgz], cwd=d)
    dest = os.path.join(d, "node_modules", name)
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    os.rename(os.path.join(d, "package"), dest)
    strip_maps(dest)
    t = {"fetch": time.time() - t0}
    manifest = json.load(open(os.path.join(dest, "package.json")))
    deps = set()
    for k in ("dependencies", "peerDependencies", "optionalDependencies"):
        deps |= set((manifest.get(k) or {}).keys())
    externals = sorted(deps) + sorted(f"{x}/*" for x in deps)
    r = build_and_print(d, name, externals)
    builds = r.pop("builds")
    r.pop("specs")
    t.update(r)
    return {"time": t, "builds": builds, "files": None, "minifyFailed": []}


def bundle_route(name, version, published, base):
    d = os.path.join(base, "bundle")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    t = {}
    json.dump({"private": True, "name": "ix-app", "dependencies": {name: version}}, open(os.path.join(d, "package.json"), "w"))
    t0 = time.time()
    npm = ["npm", "install", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error", f"--before={published}"]
    rc, msg = run(npm, cwd=d)
    if rc != 0:
        rc, msg = run(npm + ["--legacy-peer-deps"], cwd=d)
    if rc != 0:
        return {"error": "install: " + msg[:300]}
    strip_maps(os.path.join(d, "node_modules"))
    t["install"] = time.time() - t0
    t.update(build_and_print(d, name, []))
    builds, specs = t.pop("builds"), t.pop("specs")
    return {"time": t, "builds": builds, "specifiers": specs}


# How the direct route minifies each file (one Bun process per batch of files):
#   nobundle  Bun's transpiler (`--no-bundle --minify`): no bundler at all
#   single    every file its OWN bundle entry with every import external
#             (`--external '*'`), under the slice's recipe (cjs, target bun):
#             the bundler's minifier, no install, no module graph
#   single-esm  as single, ESM output format
#   pkg-bundle  the builder's two entries over the package's own tarball,
#               its declared dependencies external (no dependency install)
MODES = {
    "nobundle": ["--no-bundle", "--minify", "--target", "bun", "--define", DEFINE],
    "single": ["--minify", "--target", "bun", "--format", "cjs", "--external", "*", "--define", DEFINE],
    # the same, ESM output: a CommonJS file becomes a bundled-module factory
    # (as inside the slice's bundle) and an ES module keeps its imports as
    # plain bindings instead of `require()` + interop property reads
    "single-esm": ["--minify", "--target", "bun", "--format", "esm", "--external", "*", "--define", DEFINE],
    # not a per-file mode: pkg_bundle_route (the builder's entries over the
    # package's own tarball, its dependencies external)
    "pkg-bundle": None,
}


def direct_route(name, version, base, mode):
    d = os.path.join(base, f"direct-{mode}")
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    t = {}
    t0 = time.time()
    rc, msg = run(["npm", "pack", f"{name}@{version}", "--silent", "--ignore-scripts"], cwd=d)
    if rc != 0:
        return {"error": "pack: " + msg[:300]}
    tgz = [f for f in os.listdir(d) if f.endswith(".tgz")][0]
    rc, msg = run(["tar", "xzf", tgz], cwd=d)
    pkg = os.path.join(d, "package")
    if rc != 0 or not os.path.isdir(pkg):
        return {"error": "untar: " + msg[:300]}
    t["fetch"] = time.time() - t0
    # sponge-named files (.min., bundle., ...) are kept: the builder's unit
    # rows come from whatever file a bundle pulled in (glob's exports point
    # at dist/esm/index.min.js); only the literal pass drops sponges
    files = [f for f in walk(pkg) if is_js(f) and os.path.getsize(os.path.join(pkg, f)) <= 4 * 1024 * 1024]
    t0 = time.time()
    mind = os.path.join(d, "min")
    failed = []
    # one Bun process per batch of files; a batch that fails is redone per file
    for i in range(0, len(files), 200):
        batch = files[i : i + 200]
        cmd = [BUN, "build"] + MODES[mode]
        cmd += ["--root", pkg, "--outdir", mind] + [os.path.join(pkg, f) for f in batch]
        rc, _ = run(cmd)
        if rc != 0:
            for f in batch:
                cmd = [BUN, "build"] + MODES[mode]
                cmd += ["--root", pkg, "--outdir", mind, os.path.join(pkg, f)]
                if run(cmd)[0] != 0:
                    failed.append(f)
    t["minify"] = time.time() - t0
    t0 = time.time()
    pd = os.path.join(d, "prints")
    printed, unprinted = 0, []
    for f in files:
        # Bun keeps the relative path; .mjs/.cjs outputs are written as .js
        cands = [os.path.join(mind, f), os.path.join(mind, os.path.splitext(f)[0] + ".js")]
        src = next((c for c in cands if os.path.exists(c)), None)
        if src is None:
            if f not in failed:
                unprinted.append(f)
            continue
        out = os.path.join(pd, f + ".json")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        if discover(src, out):
            printed += 1
        else:
            unprinted.append(f)
    t["fingerprint"] = time.time() - t0
    return {"time": t, "files": len(files), "printed": printed, "minifyFailed": failed, "unprinted": unprinted[:50]}


def main():
    for line in open(PICKS):
        rank, name, version, cls = line.split("\t")[:4]
        base = os.path.join(ROOT, f"{safe(name)}@{version}")
        os.makedirs(base, exist_ok=True)
        rec = {"name": name, "version": version, "class": cls, "rank": int(rank)}
        if "bundle" in ROUTES and not os.path.exists(os.path.join(base, "timing.json")):
            pk = json.load(open(os.path.join(REGISTRY, f"{safe(name)}.json")))
            rec["published"] = pk["time"][version]
            t0 = time.time()
            rec["bundle"] = bundle_route(name, version, rec["published"], base)
            rec["bundle"]["wall"] = time.time() - t0
            json.dump(rec, open(os.path.join(base, "timing.json"), "w"), indent=1)
        for mode in MODES:
            if mode not in ROUTES:
                continue
            out = os.path.join(base, f"timing-{mode}.json")
            if os.path.exists(out):
                continue
            t0 = time.time()
            r = pkg_bundle_route(name, version, base) if mode == "pkg-bundle" else direct_route(name, version, base, mode)
            r["wall"] = time.time() - t0
            json.dump(r, open(out, "w"), indent=1)
        print(name, version, flush=True)


if __name__ == "__main__":
    main()
