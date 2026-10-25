"""Experiment A step 2: each app's production dependency closure, weighted by the bytes a bundler pulls in.

usage: python3 -I closure.py [--jobs 10] [--only <id>...]      -> $SEL/apps/<slug>/closure.json, $SEL/closures.jsonl

Per app, in $SEL/apps/<slug>/:
  install   electron / agent: package.json with the app's runtime dependency ranges (corpus.py's rule), resolved TODAY
            (not the app's lockfile). cli: the published CLI at its latest version. Both:
            `npm install --ignore-scripts --omit=dev --legacy-peer-deps` (no package code runs).
  entry     electron / agent: entry.cjs requires every runtime dependency (`require("x")`) — the libraries the app ships.
            cli: entry.cjs requires the CLI's own bin file(s) by path — exactly what a bundler pulls in from the
            real entry point (the CLI's own files are its APP code, never library bytes).
  bundle    esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --metafile. An unresolvable specifier is
            retried as --external (up to 10 rounds); an unknown file type gets the `empty` loader; top-level await
            retries as --format=esm. process.env.NODE_ENV is defined as "production" (as app bundles do; Bun
            inlines it in Claude Code).
  weight    metafile outputs[].inputs[path].bytesInOutput: the MINIFIED bytes each input file contributes to the
            bundle. A file is owned by the package under the LAST node_modules/ in its path (name + version from
            that package.json). Weight of a package in an app = the sum over its files.
"""
import json, os, re, subprocess, sys, shutil, hashlib
from concurrent.futures import ThreadPoolExecutor

SEL = os.environ.get("SEL", "/work/exp095-library-fingerprints/selection")
ESB = "/work/exp095-library-fingerprints/lab/tools/node_modules/esbuild/bin/esbuild"
NPM_FLAGS = ["install", "--ignore-scripts", "--omit=dev", "--legacy-peer-deps", "--no-audit", "--no-fund", "--loglevel=error"]


def slug(app_id):
    s = re.sub(r"[^A-Za-z0-9._-]+", "_", app_id)
    return s[:80] + "-" + hashlib.sha1(app_id.encode()).hexdigest()[:6]


def run(cmd, cwd, timeout=1800):
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, errors="replace", timeout=timeout,
                           env=dict(os.environ, GOMAXPROCS="2"))
        return p.returncode, p.stdout + p.stderr
    except subprocess.TimeoutExpired:
        return 124, "timeout"


def install(app, d):
    if os.path.exists(d + "/.installed"):
        return None
    if app["kind"] != "cli":
        deps = {k: v for k, v in app["deps"].items() if ":" not in v}  # yarn patch:/portal: and other protocols
    else:
        deps = {app["name"]: app["version"]}
    json.dump({"private": True, "name": "sel-app", "dependencies": deps}, open(d + "/package.json", "w"), indent=1)
    rc, out = run(["npm"] + NPM_FLAGS, d)
    if rc != 0 and app["kind"] != "cli":
        # one bad range sinks the whole install: drop the deps npm names as unresolvable, once
        bad = set(re.findall(r"(?:No matching version found for|404 Not Found - GET \S+/)(@?[^@\s/]+(?:/[^@\s]+)?)", out))
        bad |= {m for m in re.findall(r"notarget.*?for (@?[^@\s]+)@", out)}
        deps2 = {k: v for k, v in deps.items() if k not in bad and k.replace("/", "%2f") not in bad}
        if deps2 != deps:
            json.dump({"private": True, "name": "sel-app", "dependencies": deps2}, open(d + "/package.json", "w"), indent=1)
            rc, out = run(["npm"] + NPM_FLAGS, d)
    if rc != 0 and "EBADPLATFORM" in out:
        rc, out = run(["npm"] + NPM_FLAGS + ["--force"], d)  # a macOS/Windows-only optional dependency
    if rc != 0:
        return "install failed: " + out[-600:]
    open(d + "/.installed", "w").close()
    return None


def entry(app, d):
    lines = []
    if app["kind"] != "cli":
        for k in sorted(json.load(open(d + "/package.json"))["dependencies"]):
            lines.append(f"try{{module.exports[{json.dumps(k)}]=require({json.dumps(k)})}}catch(e){{}}")
    else:
        root = f"{d}/node_modules/{app['name']}"
        b = app["bin"] if isinstance(app["bin"], dict) else {app["name"]: app["bin"]}
        for p in sorted(set(b.values())):
            f = os.path.normpath(os.path.join(root, p))
            if os.path.exists(f):
                lines.append(f"require({json.dumps(f)});")
    open(d + "/entry.cjs", "w").write("module.exports={};\n" + "\n".join(lines) + "\n")
    return len(lines)


def unresolved(text):
    out = set(re.findall(r'Could not resolve:? "([^"]+)"', text))
    return sorted(out)


def bundle(d, tool="esbuild", out_dir=None, entry_file="entry.cjs", target="node"):
    """esbuild with metafile (weights) or bun (held-out query bundles; `target` is Bun's --target: node, or bun for
    exp150's target-bun builds); returns (ok, externals, loaders, esm, log)."""
    externals, loaders, esm = [], [], False
    last = ""
    deps = json.load(open(d + "/package.json")).get("dependencies") or {}
    protect = set(deps) if len(deps) == 1 else set()  # a CLI app: never externalize the app itself
    for _ in range(12):
        if tool == "esbuild":
            cmd = [ESB, entry_file, "--bundle", "--minify", "--platform=node", "--log-level=error", "--metafile=meta.json",
                   "--outfile=out.js", "--log-limit=0", f"--format={'esm' if esm else 'cjs'}", "--loader:.node=empty", '--define:process.env.NODE_ENV="production"']
            cmd += [f"--loader:{x}=empty" for x in loaders] + [f"--external:{x}" for x in externals]
        else:
            cmd = [tool, "build", entry_file, "--target", target, "--format", "esm" if esm else "cjs", "--minify",
                   "--sourcemap=external", "--outdir", out_dir, "--define", 'process.env.NODE_ENV="production"']
            for x in externals:
                cmd += ["--external", x]
            for x in loaders + [".node"]:
                cmd += ["--loader", f"{x}:file"]
        rc, last = run(cmd, d, timeout=1200)
        if rc == 0:
            return True, externals, loaders, esm, ""
        new = [x for x in unresolved(last) if x not in externals]
        nl = sorted({x for x in re.findall(r'No loader is configured for "(\.[^"]+)" files', last) if x not in loaders})
        # a non-literal require (glob, expression) inside a package: externalize that PACKAGE (its bytes are lost)
        bad = sorted({m for m in re.findall(r"node_modules/((?:@[^/\s]+/)?[^/\s]+)/", last) if m not in externals and m not in protect})
        if new and len(externals) < 400:
            externals += new
        elif nl:
            loaders += nl
        elif re.search(r'[Tt]op-level await|"await" can only be used inside an "async" function', last) and not esm:
            esm = True
        elif bad and len(externals) < 400:
            externals += bad
        else:
            break
    return False, externals, loaders, esm, last[-1500:]


_pj = {}


def owner(path):
    parts = path.split("/")
    if "node_modules" not in parts:
        return None
    i = len(parts) - 1 - parts[::-1].index("node_modules")
    w = 2 if i + 1 < len(parts) and parts[i + 1].startswith("@") else 1
    root = "/".join(parts[: i + 1 + w])
    if root not in _pj:
        try:
            j = json.load(open(root + "/package.json"))
            _pj[root] = (j.get("name") or "/".join(parts[i + 1 : i + 1 + w]), j.get("version") or "?")
        except Exception:
            _pj[root] = ("/".join(parts[i + 1 : i + 1 + w]), "?")
    return _pj[root]


def weigh(app, d):
    meta = json.load(open(d + "/meta.json"))
    pk, appb, other = {}, 0, 0
    for o in meta["outputs"].values():
        for path, inp in (o.get("inputs") or {}).items():
            b = inp.get("bytesInOutput", 0)
            ow = owner(os.path.normpath(os.path.join(d, path)))
            if ow is None:
                other += b
                continue
            name, ver = ow
            if app["kind"] == "cli" and name == app["name"]:
                appb += b
                continue
            e = pk.setdefault(name, {"version": ver, "bytes": 0, "files": 0})
            if e["version"] != ver and ver not in e["version"].split(","):
                e["version"] += "," + ver  # two copies of one package at different versions
            e["bytes"] += b
            e["files"] += 1
    return pk, appb, other


def one(app):
    d = f"{SEL}/apps/{slug(app['id'])}"
    os.makedirs(d, exist_ok=True)
    res = {"id": app["id"], "slug": slug(app["id"]), "kind": app["kind"]}
    if os.path.exists(d + "/closure.json"):
        return json.load(open(d + "/closure.json"))
    err = install(app, d)
    if err:
        res["error"] = err
    else:
        res["entries"] = entry(app, d)
        ok, ext, ld, esm, log = bundle(d)
        res.update(externals=ext, loaders=ld, esm=esm)
        if not ok:
            res["error"] = "bundle failed: " + log
        else:
            pk, appb, other = weigh(app, d)
            res.update(packages=pk, appBytes=appb, otherBytes=other, libBytes=sum(p["bytes"] for p in pk.values()),
                       outBytes=os.path.getsize(d + "/out.js"))
            try:
                os.remove(d + "/out.js")  # weights are in meta.json; the held-out lane rebuilds with Bun
            except OSError:
                pass
    json.dump(res, open(d + "/closure.json", "w"), indent=1)
    return res


def main():
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 10
    apps = json.load(open(SEL + "/corpus.json"))["apps"]
    if "--only" in sys.argv:
        want = set(sys.argv[sys.argv.index("--only") + 1 :])
        apps = [a for a in apps if a["id"] in want]
    with ThreadPoolExecutor(jobs) as ex, open(SEL + "/closures.jsonl", "w") as f:
        for r in ex.map(one, apps):
            f.write(json.dumps({k: v for k, v in r.items() if k != "packages"} | {"npkg": len(r.get("packages") or {})}) + "\n")
            f.flush()
            print(r["id"], r.get("error", "")[:120] or f"{len(r.get('packages') or {})} pkgs {r.get('libBytes')} B", flush=True)


if __name__ == "__main__":
    main()
