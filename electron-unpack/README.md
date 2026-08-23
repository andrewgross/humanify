# electron-unpack — harvesting Electron app source for humanify

> **STATUS (2026-08-23):** The extraction chain is **proven** on ZCode 3.8.1
> (linux-arm64 `.deb`) with the exact commands in `harvest.sh`, and **humanify
> now consumes the output directly**: pointing it at an extracted app directory
> selects the electron pipeline pieces (see "Running humanify on the output"
> below). The acquisition library (all-versions resolver/fetcher) is still
> design-only — this doc is its map.

## Running humanify on the output

Two steps, exactly like the Bun flow (extract upfront, then humanify):

```bash
./harvest.sh 3.8.1 linux-x64 ./corpus          # installer → app.asar → files
humanify ./corpus/zcode/3.8.1/linux-x64/app --api-key ... -o ./out-3.8.1
```

humanify detects the directory as an extracted Electron app
(`detectElectronApp`: package.json `main` resolves + electron markers), routes
it to the electron unpack adapter, copies the app's own JS (everything under
the main entry's top-level dir — `out/` here; `node_modules` stays behind and
is recorded as a vendored census in `.humanify/electron-app.json`), and renames
each file in place. An `app.asar` given directly is refused with the extraction
command; a directory that is not app-shaped fails loudly upfront.

Not yet supported for directory input (each fails loudly, none half-work):
`--split` (the app is already a file tree), `--prior-version` (per-file prior
matching across content-hashed chunk names is the next build — humanify's
matcher territory, per Part 4), and the last-file-only artifact flags
(`--stats-json`, `--diagnostics`, `--rename-ledger`).

## What we want

Feed **successive released versions** of an Electron app's JavaScript into
humanify's cross-version pipeline (version N-1 → N via `--prior-version`). To do
that we need, for each version, a clean tree of the app's own minified JS,
separated from third-party libraries, with stable provenance (version, build
commit, hashes).

First target: **ZCode** (Z.ai's coding harness). Later: Discord, Slack, and other
Electron apps. So the design is a small pluggable pipeline, not a one-off scraper.

The unpacker is a **separate library** whose _output_ humanify consumes — the same
split the user asked for ("something like bun decompile"). humanify stays a pure
deobfuscator; electron-unpack owns acquisition + extraction.

---

## Part 1 — The extraction chain, proven on ZCode 3.8.1

Every number below is measured from the real `linux-arm64` package.

```
ZCode-3.8.1-linux-arm64.deb        126 MB   (ar archive: debian-binary + control.tar.xz + data.tar.xz)
  └─ data.tar.xz → rootfs/
       opt/ZCode/resources/
         app.asar                  288 MB   26,811 entries  → extracts to 342 MB / 23,944 files
         app.asar.unpacked         152 KB   (native .node addons, pulled out of the asar)
         tools/ config/ model-providers/ glm/   ← extra resources OUTSIDE the asar
```

Inside `app.asar`:

| Tree             | Size   | What it is                                                      | humanify?                                                              |
| ---------------- | ------ | --------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `/out/main`      | 1.7 MB | Electron **main** process (esbuild, code-split `chunk-*.js`)    | **yes**                                                                |
| `/out/host`      | 2.5 MB | host process bundle                                             | **yes**                                                                |
| `/out/preload`   | 1.8 MB | preload scripts (`.cjs`, ~12k chars/line)                       | **yes**                                                                |
| `/out/scheduler` | 948 KB | scheduler entry                                                 | **yes**                                                                |
| `/out/renderer`  | 49 MB  | React 19 UI (Vite, content-hashed assets)                       | **partly** — app UI yes; the shiki/mermaid grammar chunks are vendored |
| `/node_modules`  | 286 MB | 441 third-party packages, shipped with their own `package.json` | **no** — identify & skip                                               |

**Clean boundary:** the app's own code is entirely under **`/out`** (~56 MB, 790
JS files). First-party `@zcode/*` workspace packages are _bundled into `/out`_,
not shipped under `node_modules` — so `/out` vs `node_modules` is exactly the
app-vs-library split humanify's library-detection stage already wants.

**It is genuinely minified — humanify is needed.** No `.map` files anywhere. Mangled
single-char identifiers, cross-chunk import aliasing:

```js
// out/main/index.js  (1.06 MB, 1336 lines, ~795 chars/line)
import{a as BC}from"./chunk-KHSEGNJU.js";import{b as LC,c as NC}from"./chunk-P2LTMAMU.js";import{$ as _r,$a as cC,...
```

**`out/metadata/build-meta.json` is provenance gold:**

```json
{
  "appVersion": "3.8.1",
  "buildCommitId": "99f955e2",
  "buildTime": "2026-08-18T14:34:54.787Z",
  "electronBuilderVersion": "26.8.1"
}
```

The build commit id + time anchor each version and tell us the true adjacency of
two builds — better than trusting the semver alone.

### The tooling gotcha

macOS BSD `ar` mangles GNU-format `.deb` member names (emits `data.tar.xz/` with a
trailing slash and then can't extract it). **Use `bsdtar` (libarchive)** — it reads
the `ar` container, `xz`, and `zstd` in one tool on every platform. `dpkg-deb` also
works where present. `ar` is the trap.

---

## Part 2 — Getting _all_ the versions

**URL pattern (confirmed 200 for 3.8.1, 3.7.7, 3.6.5, 3.5.3, 3.4.2 × all 4 platforms):**

```
https://cdn-zcode.z.ai/zcode/electron/releases/{version}/{os}-{arch}/ZCode-{version}-{tok}.{ext}

  os-arch          tok          ext
  linux-x64        linux-x64    deb | AppImage
  linux-arm64      linux-arm64  deb | AppImage
  macos-x64        mac-x64      dmg
  macos-arm64      mac-arm64    dmg
  windows-x64      win-x64      exe
  windows-arm64    win-arm64    exe
```

So once we have the **version list**, every artifact is a deterministic URL.

**Version list source** — the CDN is an Alibaba OSS bucket behind a Tengine CDN;
directory listing is **403** (no enumeration by listing). Two working sources:

1. **The changelog page** `https://zcode.z.ai/en/changelog` lists every release
   (3.8.1, 3.7.7, 3.7.6, 3.7.5, 3.6.5, 3.5.3, 3.5.2, 3.4.2, … "scroll to load more").
   It's a SPA, so scrape the backing JSON endpoint (find it in devtools/HAR) rather
   than the rendered HTML.
2. **The electron-updater feed** exists — `…/{version}/windows-x64/latest.yml` is a
   200 (standard electron-builder manifest with version + sha512 + size). It gives
   the _current_ version per channel and its checksums; good for integrity + "is
   there a newer one" but not full history.

**Recommendation:** treat the changelog as the version index, verify each guessed
URL with a cheap `HEAD`, and record `sha512` from `latest.yml` where available.

**One platform is enough for the version walk.** The JS bundles should be identical
across platforms for a given version (same Vite/esbuild output; only native `.node`
addons and a few `process.platform` branches differ). Pick **`linux-x64` `.deb`** as
the canonical walk target — it cracks with `bsdtar` on any OS, no dmg/NSIS needed.
Download other platforms only if we specifically want platform-divergent code.
_(Verify the identical-across-platforms assumption once by diffing `/out` of
`linux-x64` vs `macos-arm64` for the same version — see Open questions.)_

---

## Part 3 — Generic installer → asar matrix

The per-app resolver differs, but "installer → Electron app dir → asar → files" is
generic. Extractor by artifact type:

| Format          | OS produced for | Extract with                                                       | Notes                                                                                    |
| --------------- | --------------- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| `.deb`          | Linux           | `bsdtar` / `dpkg-deb -x`                                           | `ar` → `data.tar.xz` → rootfs. **not** BSD `ar`.                                         |
| `.AppImage`     | Linux           | `./x.AppImage --appimage-extract`, or `unsquashfs` on the offset   | it's an ELF + squashfs; needs `squashfs-tools` (no FUSE needed for `--appimage-extract`) |
| `.rpm`          | Linux           | `bsdtar` / `rpm2cpio \| cpio`                                      | same payload as deb                                                                      |
| `.dmg`          | macOS           | `hdiutil attach` (mac), `7z` / `dmg2img`+`darling-dmg` (elsewhere) | mount → copy `*.app/Contents/Resources` → detach                                         |
| `.zip` (mac)    | macOS           | `unzip` / `bsdtar`                                                 | contains the `.app` bundle directly                                                      |
| `.exe` NSIS     | Windows         | `7z x`                                                             | most electron-builder Windows installers are NSIS                                        |
| `.exe` Squirrel | Windows         | `7z x` the embedded `.nupkg`                                       | Discord/Slack/VS Code style                                                              |

Then, uniformly:

```
find <appdir> -name '*.asar'          # app.asar, plus secondary asars (see Part 6)
npx @electron/asar extract app.asar out/
# also sweep app.asar.unpacked + sibling resource dirs for loose JS/config
```

**Locating the app dir** by OS layout: `.app/Contents/Resources` (mac), `/opt/<App>/resources`
or `/usr/lib/<app>/resources` (linux deb), `resources/` next to the `.exe` after NSIS
extract (windows).

---

## Part 4 — What humanify actually ingests, and the cross-version wrinkle

**Target set per version:** the `/out` tree (~56 MB, 790 files here) minus the
vendored renderer chunks. **Skip set:** `/node_modules` (441 packages, identifiable
by their own `package.json` name+version) and the shiki/mermaid/theme grammar chunks
in `renderer/assets` (≈hundreds of files — vendored, not app code).

**Cross-version file identity is the real challenge, and it's humanify's home turf.**
Vite/esbuild **content-hash** chunk and asset filenames (`button-CNDcftUz.js`,
`chunk-KHSEGNJU.js`), so _filenames churn on every build_. Only the entry points
(`out/main/index.js`, `out/preload/index.cjs`, …) are stable. This is precisely the
situation humanify's split-inheritance + call-graph fingerprint matching is built for
— match functions across versions by structure, not by filename. **Implication for
electron-unpack:** preserve the entry-point layout and hand humanify the raw hashed
names; do **not** try to stabilize filenames ourselves (that's humanify's job, and
doing it here would fight the matcher).

**Output contract** (what a version dir looks like to humanify):

```
<app>/<version>/
  app/            # the humanify target — /out, verbatim (minified)
  vendor.json     # detected libraries: [{name, version, path}] from node_modules package.jsons
  skip.txt        # vendored renderer chunks to exclude
  manifest.json   # {app, version, buildCommitId, buildTime, electron/​builderVersion,
                  #  platform, artifactSha512, fileCount, bytes}
```

humanify then walks `<app>/<v-1>/app` → `<app>/<v>/app` with `--prior-version`.

---

## Part 5 — Proposed library shape

A small pipeline of pluggable stages (mirrors humanify's own strategy-registry
style; each stage is swappable per app/format):

```
resolve → fetch → unwrap → locate → unpack → classify → manifest
```

1. **resolve** `(app, version?, platform, arch) → [artifactDescriptor]`
   _Per-app plugin._ Owns version discovery (changelog/API) + URL templating.
   `resolveLatest()` and `resolveAll()`. ZCode plugin ≈ 30 lines.
2. **fetch** `descriptor → cachedFile` — content-addressed cache keyed by sha512
   (from `latest.yml` when present), resumable, HEAD-verified. Hermetic re-runs.
3. **unwrap** `installer → appDir` — **format registry** keyed by extension
   (`.deb/.AppImage/.dmg/.exe/.zip`). This is the cross-platform muscle.
4. **locate** `appDir → {asars[], unpackedDir, looseResources[]}` — sweeps for
   `app.asar` **and** secondary asars (`core.asar`) and loose JS/config.
5. **unpack** `asar → files` — `@electron/asar` extract.
6. **classify** `files → {appCode, vendored}` — `/out`-style app tree vs
   `node_modules`/known-library chunks; emit `vendor.json` + `skip.txt`.
7. **manifest** — write the version dir + `manifest.json` (Part 4 contract).

Language: **TypeScript**, to match humanify and be importable by it. Ships a CLI
(`electron-unpack harvest zcode --all --platform linux-x64 --out ./corpus`) and a
programmatic API (`harvest({app, version, platform}) → versionDir`).

`harvest.sh` in this folder is a throwaway shell proof of stages 1–5 for ZCode; the
TS package is the real thing.

---

## Part 6 — Generalizing to Discord / Slack / VS Code

The pipeline holds; the wrinkles live in **locate** and **resolve**:

- **Discord** — `app.asar` is a _thin bootstrap loader_; the real code is a
  **separately hot-downloaded `core.asar`** under
  `…/modules/discord_desktop_core-*/discord_desktop_core/core.asar` (Windows
  installer is Squirrel). The **locate** stage must follow into `modules/` and
  handle multiple asars, and resolve must know Discord's build feed. Same shape for
  many auto-updating apps.
- **Slack** — the easy case: a single `app.asar` at `…/Resources/app.asar`
  (`/usr/lib/slack/resources/app.asar` on linux). Standard unpack.
- **VS Code (and VS Code-derived harnesses)** — often **no asar**; source sits
  unpacked at `resources/app/out/vs/**`. **locate** must handle the "already
  unpacked" case (skip stage 5). Worth flagging because "coding harness" often
  means VS Code lineage — though ZCode itself is _not_ VS Code-based (it's
  electron-vite + React 19).

So: one core pipeline, per-app **resolve** plugins, and a **locate** stage smart
enough for (single asar) / (bootstrap + core asar) / (no asar, pre-unpacked).

---

## Part 7 — Off-the-shelf vs build

**Reuse (don't build):**

- `@electron/asar` — the asar list/extract standard. ([electron/asar](https://github.com/electron/asar))
- `bsdtar`/libarchive — deb(ar)+xz+zstd+zip+rpm in one tool, cross-platform.
- `hdiutil` (mac), `7z`/`p7zip` (NSIS/Squirrel/zip), `squashfs-tools` (AppImage),
  `rpm2cpio` (rpm).
- `undici`/`fetch` for downloads; electron-updater `latest*.yml` for checksums.

**Build (the actual work):**

- Per-app **resolve** plugins (version discovery + URL templates) — no generic tool
  exists for "enumerate every released version of an Electron app."
- The **unwrap** format registry as a portable Node API over the extractors above.
- **classify** (app vs vendored) and the **manifest**/output contract humanify reads.
- Orchestration + content-addressed cache + the CLI.

There are one-off blog recipes for "decompile one Electron app" and GUI asar
tools, but nothing that harvests a _version series_ into a humanify-ready corpus.
That series-oriented, provenance-tracked harvester is what's novel here.

---

## Open questions / next steps

1. **Validate cross-version diff** — download ZCode 3.7.7 (adjacent to 3.8.1),
   extract `/out`, and confirm (a) entry points are stably named, (b) chunk/asset
   names churn as predicted, (c) humanify matches across them. This is the real
   proof the "subsequent versions" idea works end-to-end.
2. **Confirm platform-independence of the JS** — diff `/out` of `linux-x64` vs
   `macos-arm64` for one version; if ~identical, lock the walk to `linux-x64`.
3. **Find the changelog's JSON endpoint** for a complete, scriptable version list
   (the rendered page paginates by scroll).
4. **Decide the classify granularity for the renderer** — how aggressively to skip
   shiki/mermaid chunks vs let humanify's library detection handle them.
5. **Then build the TS package** (Part 5), starting with the ZCode resolver.
