"""Experiment A step 1: the corpus of bundled apps.

usage: python3 -I corpus.py            -> $SEL/corpus.json, $SEL/corpus-log.json

Two strata, chosen by rule (README "How the corpus was chosen"):

  electron  every app in the electron/apps catalog (github.com/electron/apps, main branch, apps/*/*.yml) whose
            repository is on GitHub; ranked by stars from ecosyste.ms (repos.ecosyste.ms hosts/GitHub lookup);
            package.json read from the default branch (raw.githubusercontent.com). Kept: not archived, not a fork,
            electron in its (dev)dependencies, >= 3 runtime library dependencies after the rule below. Top
            ELECTRON_N by stars.
            Runtime dependencies = `dependencies` (minus electron itself and tooling), PLUS `devDependencies` minus
            tooling when the app bundles its code (a bundler is in devDependencies: webpack, vite, electron-vite,
            esbuild, rollup, parcel, @electron-forge/plugin-webpack/vite) — bundled Electron apps routinely list
            renderer libraries as devDependencies.
  cli       npm CLIs: packages from npm-high-impact's top-download list whose LATEST manifest has a `bin` and
            >= 5 runtime dependencies (a CLI with its own dependency tree, i.e. an app a bundler could ship as one
            file), top CLI_N by download rank; PLUS a hand list of product / agent CLIs (CURATED) with a bin and
            >= 3 dependencies. Pre-bundled CLIs (0-2 deps) are recorded and dropped: their libraries are invisible.
            The CLI is installed from npm and its bin entry is bundled, so the closure is exactly what a bundler
            pulls in from the real entry point.
"""
import json, os, re, sys, glob, subprocess
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from net import get, SEL

ELECTRON_N, CLI_N = 200, 140
X = "/work/exp095-library-fingerprints"
TOP_JS = X + "/exact-module-hash/npmtop/node_modules/npm-high-impact/index.js"

TOOL = re.compile(
    r"^(@types/|@typescript-eslint/|eslint|@eslint/|prettier|@biomejs/|jest|@jest/|ts-jest|vitest|@vitest/|mocha|chai|sinon|nyc|c8|karma|"
    r"jasmine|ava$|tap$|@testing-library/|playwright|@playwright/|puppeteer|spectron|webdriverio|@wdio/|cypress|"
    r"webpack|@webpack|vite$|vite-|@vitejs/|electron-vite|esbuild|rollup|@rollup/|parcel|@parcel/|rspack|@rspack/|swc|@swc/|"
    r"babel|@babel/|typescript$|ts-node|tsx$|tsup|terser|uglify|ts-loader|.*-loader$|sass$|node-sass|less$|stylus|postcss|"
    r"autoprefixer|tailwindcss|@tailwindcss/|cssnano|husky|lint-staged|commitizen|@commitlint/|cz-|standard-version|"
    r"semantic-release|@semantic-release/|cross-env|rimraf|concurrently|npm-run-all|nodemon|wait-on|electron$|electron-builder|"
    r"electron-packager|@electron/|@electron-forge/|electron-rebuild|electron-devtools|electron-notarize|electron-icon|"
    r"electron-reload|electron-reloader|devtron|node-gyp|prebuild|storybook|@storybook/|chromatic|lerna|nx$|@nx/|turbo$|"
    r"copyfiles|cpy-cli|shx|del-cli|gulp|grunt|browserify|watchify|patch-package|@vue/cli|@vue/test-utils|vue-loader|"
    r"vue-template-compiler|@angular-devkit/|@angular/cli|@angular/compiler-cli|ng-packagr|react-scripts|react-app-rewired|"
    r"@craco/|@pmmmwh/|react-refresh|svelte-check|svelte-preprocess|@sveltejs/vite|@sveltejs/kit|unplugin|vue-tsc|"
    r"@electron-toolkit/tsconfig|@electron-toolkit/eslint|dotenv-cli|env-cmd|madge|depcheck|knip|license-checker|"
    r"cpx|ncp|mkdirp$|chokidar-cli|html-webpack|mini-css|css-loader|style-loader|file-loader|url-loader|fork-ts-checker|"
    r"@sentry/webpack-plugin|@sentry/cli|@sentry/vite-plugin|vue-cli-plugin|@quasar/app|quasar$|nuxt$|@nuxt/|next$|"
    r"@next/|electron-log-viewer|asar$|@electron/asar|dmg-|appdmg|7zip|png2icons|icon-gen|sharp-cli)")
BUNDLERS = re.compile(r"^(webpack|vite|electron-vite|esbuild|rollup|parcel|@parcel/core|@electron-forge/plugin-(webpack|vite)|electron-webpack|@rspack/core|electron-next|electron-react-boilerplate)$")

CURATED = [
    "@google/gemini-cli", "@qwen-code/qwen-code", "@sourcegraph/amp", "opencode-ai", "@openai/codex", "@githubnext/github-copilot-cli",
    "@github/copilot", "@vibe-kit/grok-cli", "@kilocode/cli", "@charmland/crush", "@augmentcode/auggie", "task-master-ai",
    "vercel", "netlify-cli", "firebase-tools", "wrangler", "@shopify/cli", "@expo/cli", "eas-cli", "@angular/cli",
    "@nestjs/cli", "aws-cdk", "serverless", "@railway/cli", "heroku", "@salesforce/cli", "@azure/static-web-apps-cli",
    "@google/clasp", "renovate", "lerna", "@capacitor/cli", "@ionic/cli", "cordova", "@react-native-community/cli",
    "gatsby-cli", "@vue/cli", "@nuxt/cli", "create-react-app", "@storybook/cli", "@sanity/cli", "contentful-cli",
    "@prisma/cli", "prisma", "@redocly/cli", "@stoplight/spectral-cli", "@asyncapi/cli", "@apidevtools/swagger-cli",
    "newman", "artillery", "lighthouse", "@lhci/cli", "pm2", "forever", "http-server", "serve", "localtunnel", "ngrok",
    "json-server", "nodemon", "npm-check-updates", "np", "release-it", "changelogen", "@changesets/cli", "verdaccio",
    "yo", "plop", "hygen", "degit", "tldr", "fkill-cli", "speed-test", "fast-cli", "carbon-now-sh", "terminalizer",
    "@mermaid-js/mermaid-cli", "markdownlint-cli", "markdownlint-cli2", "@marp-team/marp-cli", "docsify-cli", "@docusaurus/core",
    "vuepress", "hexo-cli", "@11ty/eleventy", "astro", "@cloudflare/wrangler", "@aws-amplify/cli", "@vercel/ncc",
    "snyk", "@microsoft/rush", "@bitwarden/cli", "@sap/cds-dk", "@ui5/cli", "@appland/appmap", "@cyclonedx/cyclonedx-npm",
    "zx", "@withgraphite/graphite-cli", "@devcontainers/cli", "@railway/cli", "@supabase/cli", "supabase", "@mintlify/cli",
    "mintlify", "@n8n/cli", "n8n", "@builder.io/cli", "@nhost/cli", "@slack/cli", "@twilio/cli", "twilio-cli", "@sentry/wizard",
    "@stripe/cli", "@datadog/datadog-ci", "@percy/cli", "@applitools/eyes-cli", "appium", "detox-cli", "@electron-forge/cli",
]


def electron_apps():
    out, log = [], []
    for y in sorted(glob.glob(SEL + "/raw/electron-apps/apps/*/*.yml")):
        m = re.search(r"^repository:\s*'?\"?(\S+?)'?\"?\s*$", open(y, encoding="utf8", errors="replace").read(), re.M)
        if not m:
            continue
        g = re.match(r"https?://(www\.)?github\.com/([^/]+)/([^/#?]+)", m.group(1))
        if not g:
            continue
        out.append((os.path.basename(os.path.dirname(y)), f"{g.group(2)}/{g.group(3).removesuffix('.git')}"))
    rows = []
    for slug, full in out:
        st, r = get("https://repos.ecosyste.ms/api/v1/hosts/GitHub/repositories/" + full.replace("/", "%2F"), min_interval=0.3)
        if st != 200 or not isinstance(r, dict):
            log.append({"app": slug, "repo": full, "drop": f"ecosystems {st}"})
            continue
        if r.get("archived") or r.get("fork"):
            log.append({"app": slug, "repo": full, "drop": "archived or fork"})
            continue
        rows.append((r.get("stargazers_count") or 0, slug, r["full_name"], r.get("default_branch") or "master", r.get("pushed_at")))
    rows.sort(key=lambda x: (-x[0], x[1]))
    apps = []
    for stars, slug, full, branch, pushed in rows:
        if len(apps) >= ELECTRON_N:
            break
        st, pj = get(f"https://raw.githubusercontent.com/{full}/{branch}/package.json", min_interval=0.1)
        if st != 200 or not isinstance(pj, dict):
            log.append({"app": slug, "repo": full, "drop": f"no root package.json ({st})"})
            continue
        deps, dev = pj.get("dependencies") or {}, pj.get("devDependencies") or {}
        if "electron" not in deps and "electron" not in dev:
            log.append({"app": slug, "repo": full, "drop": "no electron dependency at root"})
            continue
        bundles = any(BUNDLERS.match(d) for d in list(dev) + list(deps))
        rt = {k: v for k, v in deps.items() if not TOOL.match(k)}
        if bundles:
            rt.update({k: v for k, v in dev.items() if not TOOL.match(k) and k not in rt})
        rt = {k: v for k, v in rt.items() if isinstance(v, str) and not re.match(r"^(file:|link:|workspace:|portal:|npm:|git|github:|https?:|\.|/)", v)
              and not re.match(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(#.*)?$", v)}
        if len(rt) < 3:
            log.append({"app": slug, "repo": full, "drop": f"< 3 runtime deps ({len(rt)})"})
            continue
        apps.append({"id": "electron:" + slug, "kind": "electron", "repo": full, "branch": branch, "stars": stars, "pushed": pushed,
                     "bundles": bundles, "deps": dict(sorted(rt.items()))})
    return apps, log


def top_list():
    js = f"import({json.dumps(TOP_JS)}).then(m=>console.log(JSON.stringify(m.npmTopDownloads)))"
    return json.loads(subprocess.run(["node", "-e", js], capture_output=True, text=True, check=True).stdout)


def latest(name):
    st, m = get("https://registry.npmjs.org/" + name.replace("/", "%2F") + "/latest")
    return m if st == 200 and isinstance(m, dict) else None


def cli_apps(top):
    rank = {n: i + 1 for i, n in enumerate(top)}
    cands = [n for n in top[:6000] if not re.match(r"^@types/", n)]
    with ThreadPoolExecutor(8) as ex:
        metas = dict(zip(cands, ex.map(latest, cands)))
    with ThreadPoolExecutor(8) as ex:
        cmetas = dict(zip(CURATED, ex.map(latest, CURATED)))
    apps, log, seen = [], [], set()

    def add(name, m, stratum, mindeps):
        if not m or name in seen:
            return
        deps = m.get("dependencies") or {}
        if not m.get("bin"):
            log.append({"app": name, "drop": "no bin"})
            return
        if m.get("deprecated"):
            log.append({"app": name, "drop": "deprecated"})
            return
        if len(deps) < mindeps:
            log.append({"app": name, "drop": f"pre-bundled or tiny ({len(deps)} deps)", "stratum": stratum})
            return
        seen.add(name)
        apps.append({"id": "cli:" + name, "kind": "cli", "stratum": stratum, "name": name, "version": m["version"],
                     "rank": rank.get(name), "bin": m["bin"], "ndeps": len(deps)})
    for n in CURATED:
        add(n, cmetas.get(n), "curated", 3)
    k = 0
    for n in cands:
        if k >= CLI_N:
            break
        before = len(apps)
        add(n, metas.get(n), "top-download", 5)
        k += len(apps) > before
    return apps, log


def main():
    top = top_list()
    json.dump(top, open(SEL + "/npm-top.json", "w"))
    e, elog = electron_apps()
    c, clog = cli_apps(top)
    json.dump({"electron": len(e), "cli": len(c), "apps": e + c}, open(SEL + "/corpus.json", "w"), indent=1)
    json.dump({"electron": elog, "cli": clog}, open(SEL + "/corpus-log.json", "w"), indent=1)
    print("electron", len(e), "cli", len(c), "(curated", sum(a.get("stratum") == "curated" for a in c), ")")


if __name__ == "__main__":
    main()
