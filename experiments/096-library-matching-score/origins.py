"""exp096 origin-aware truth: for each mapped byte of a source-mapped bundle, the ORIGIN package when the shipping
package carries a recognisable copy of another package's code (a vendored or pre-bundled copy).

A source map credits every byte to the file it came from, so a copy of zod inside @anthropic-ai/claude-agent-sdk's
`sdk.mjs` is labelled claude-agent-sdk (the SHIPPER), while a matcher that names it `zod` names its ORIGIN. The
truth says both. Two kinds of evidence, both read from the shipper's own published bytes (the map's
sourcesContent and paths, frozen with the corpus), never from the index or the matcher, so the predicate cannot be
satisfied by the thing it judges:

  path    the mapped file sits in a vendored directory inside the shipping package:
          `<shipper>/.../(third_party|third-party|vendor|vendored|_vendor|bundled|external)/<origin>/...`
          (puppeteer-core's lib/third_party/rxjs, @sentry/server-utils' build/esm/vendored/esquery)
  marker  the shipping file is itself a bundle that kept its bundler's module markers, and the mapped line lies
          in a region whose marker names `node_modules/<origin>/...`:
          esbuild `// ../node_modules/zod/v4/core/core.js`, rolldown/tsdown `//#region node_modules/.pnpm/...`,
          webpack `/***/ "./node_modules/x/..."`. A marker naming the shipper itself, a non-node_modules path
          (`// src/x.ts`) or `//#endregion` ends the copy.

NOT evidence (measured, README STATUS): a whole-unit hash found in the index under another package. It is the
matcher's own evidence (circular), it cannot tell an origin from a consumer that bundled the shipper (delaunator
named d3-delaunay), and it covered only ~40 KB of the unexplained wrong bytes.
Missed by design (hand check: 5 of 49 copies): copies with no marker and no vendored directory (rollup output
such as openpgp's tweetnacl, @ai-sdk/provider-utils' src/to-json-schema/zod3-to-json-schema, @sentry/node-core's
proxy/ copy of agent-base, balanced-match republished as @isaacs/balanced-match)."""
import bisect
import re

SCHEMA = "exp096-origins/1"
VENDOR_DIR = re.compile(
    r"(?:^|/)(?:third[_-]party|vendor|vendored|_vendor|bundled|external)/(@[^/]+/[^/]+|[^/@][^/]*)/")
NODE_MODULES_PKG = re.compile(r"node_modules/(@[^/\s]+/[^/\s]+|[^/@\s][^/\s]*)/")
MARKERS = [
    re.compile(r"^// (\S+)$"),                    # esbuild
    re.compile(r"^//#region (\S+)$"),             # rolldown / tsdown
    re.compile(r"^/\*\*\*/ \"(\S+)\""),           # webpack (dev)
    re.compile(r"^/\*!\*+\s*!\*\*\* (\S+) \*"),   # webpack banner
]
REGION_END = re.compile(r"^//#endregion")
PATHLIKE = re.compile(r"^(?:\.{0,2}/)*[\w@.-]+(?:/[\w@.+-]+)*\.(?:[cm]?[jt]sx?|json)$")


def marker_regions(content):
    """[(first line, origin package or None)] for every module marker in a shipped file, in line order."""
    out = []
    for i, line in enumerate(content.split("\n")):
        if REGION_END.match(line):
            out.append((i, None))
            continue
        for rx in MARKERS:
            m = rx.match(line)
            if m and PATHLIKE.match(m.group(1)):
                pkgs = NODE_MODULES_PKG.findall(m.group(1))
                out.append((i, pkgs[-1] if pkgs else None))
                break
    return out


# ---------------------------------------------------------------- scorer 7 (exp112): two more copy shapes

# Each rule can be switched off; both off, Origins builds exactly scorer 4-6's layer (verify_s6.py).
#   lazybody  rolldown/tsdown emits a lazily initialised module as its region (hoisted declarations) followed,
#             AFTER `//#endregion`, by the module's body `__esmMin(() => {...})()`; the body is the region's
#             module (up to the next marker or the file's top-level `export`), not the shipper's own code
#   chunk     a file a package built from ANOTHER package is named after it: rolldown/tsdown/unbuild chunk names
#             (`dist/jiti-Bb1dT6Cw.mjs`, `dist/_chunks/libs/cjs-module-lexer.mjs`); the file (a mapped source of
#             the shipper, or the file a marker region names inside an origin) is a copy of P when P is the
#             longest `-`-prefix of the file's stem AND the owning package declares P as a BUILD input: its npm
#             manifest's devDependencies / bundled dependencies, or its tarball's own third-party license
#             collection (`dist/licenses/<P>/LICENSE`, a LICENSE.md `## <P>` section) — and never as a runtime
#             dependency (a runtime dependency is installed beside it: a file named after one is a wrapper)
RULES7 = {"lazybody": True, "chunk": True}
LAZY = re.compile(r"^\s*(?:(?:var|let|const) [\w$]+ = )?__esm(?:Min)?\(")
EXPORT = re.compile(r"^export\b")
SRC_EXT = re.compile(r"\.(?:[cm]?[jt]sx?)$")
PNPM_PKG = re.compile(r"\.pnpm/((?:@[^/@+]+\+)?[^/@]+)@([^/_(]+)")
RUNTIME_KEYS = ("dependencies", "peerDependencies", "optionalDependencies")
BUILD_KEYS = ("devDependencies", "bundleDependencies", "bundledDependencies")


def marker_regions7(content, lazybody):
    """[(first line, origin package or None, marker path or None, kind)]: marker_regions (kind "marker"), plus
    (lazybody) each region's lazy body after its `//#endregion` (kind "lazybody")."""
    out = []
    lines = content.split("\n")
    last = None  # (origin, path) of the last region opened
    lazy = False
    for i, line in enumerate(lines):
        if REGION_END.match(line):
            out.append((i, None, None, None))
            if lazybody and last and last[0] and i + 1 < len(lines) and LAZY.match(lines[i + 1]):
                out.append((i + 1, last[0], last[1], "lazybody"))
                lazy = True
            last = None
            continue
        if lazy and EXPORT.match(line):
            out.append((i, None, None, None))
            lazy = False
            continue
        for rx in MARKERS:
            m = rx.match(line)
            if m and PATHLIKE.match(m.group(1)):
                pkgs = NODE_MODULES_PKG.findall(m.group(1))
                last = (pkgs[-1] if pkgs else None, m.group(1))
                out.append((i, last[0], m.group(1), "marker"))
                lazy = False
                break
    return out


def path_version(path, pkg):
    """The version a pnpm store path spells for pkg (`.pnpm/<name>@<version>_<peers>/`), else None."""
    for name, ver in PNPM_PKG.findall(path):
        if name.replace("+", "/") == pkg:
            return ver
    return None


def chunk_candidates(rest):
    """Package names a built file's path could be named after, longest first: its stem's `-`-prefixes."""
    base = SRC_EXT.sub("", rest.rsplit("/", 1)[-1])
    parts = base.split("-")
    return ["-".join(parts[:k]) for k in range(len(parts), 0, -1) if parts[0]]


def chunk_origin(owner, version, rest, inputs, same_family):
    """P when `rest` (a file path inside owner's package) is a chunk named after P and owner@version declares P as
    a build input and not a runtime dependency. `inputs(owner, version)` -> (runtime names, build-input names)."""
    runtime, build = inputs(owner, version)
    if not build:
        return None
    for c in chunk_candidates(rest):
        if c in build and c not in runtime and c != owner and not same_family(c, owner):
            return c
    return None


class Origins:
    """origin(source index, original line) -> (kind, package) or None, for one map's sources.
    `rules` (scorer 7, RULES7 keys; None = all off) adds the lazybody and chunk shapes; `chunk` needs `versions`
    (the truth's version per source), `inputs(pkg, version)` -> (runtime dependency names, build-input names), and
    `same_family(a, b)`."""

    def __init__(self, paths, contents, shippers, rules=None, versions=None, inputs=None, same_family=None):
        self.whole, self.regions = {}, {}
        rules = rules or {}
        contents = contents or [None] * len(paths)
        for si, (p, c) in enumerate(zip(paths, contents)):
            ship = shippers[si]
            inside = p.rsplit("node_modules/", 1)[-1] if "node_modules/" in p else p
            m = VENDOR_DIR.search(inside)
            if m and m.group(1) != ship:
                self.whole[si] = ("path", m.group(1))
                continue
            if not rules.get("lazybody") and not rules.get("chunk"):
                if c:
                    r = [(ln, ("marker", o) if o is not None and o != ship else None) for ln, o in marker_regions(c)]
                    if any(o for _, o in r):
                        self.regions[si] = ([x[0] for x in r], [x[1] for x in r])
                continue
            r = []
            if c:
                for ln, o, mp, kind in marker_regions7(c, rules.get("lazybody")):
                    if o is None or o == ship:
                        r.append((ln, None))
                        continue
                    if rules.get("chunk"):
                        rest = mp.rsplit("node_modules/" + o + "/", 1)[-1] if "node_modules/" + o + "/" in mp else ""
                        nested = chunk_origin(o, path_version(mp, o), rest, inputs, same_family) if rest else None
                        if nested:
                            r.append((ln, ("chunk", nested)))
                            continue
                    r.append((ln, (kind, o)))
            if any(o for _, o in r):
                self.regions[si] = ([x[0] for x in r], [x[1] for x in r])
            elif rules.get("chunk") and "node_modules/" in p and ship and versions:
                rest = p.rsplit("node_modules/" + ship + "/", 1)[-1] if "node_modules/" + ship + "/" in p else ""
                o = chunk_origin(ship, versions[si], rest, inputs, same_family) if rest else None
                if o:
                    self.whole[si] = ("chunk", o)

    def at(self, si, oline):
        if si in self.whole:
            return self.whole[si]
        r = self.regions.get(si)
        if r is None:
            return None
        k = bisect.bisect_right(r[0], oline) - 1
        return r[1][k] if k >= 0 else None



def segments(text, smap, inner=None):
    """truth.py `segments` with identical output (column = code point index) but linear on long non-ASCII lines
    (a minified bundle is a few very long lines; truth.py re-encodes the line prefix per mapping)."""
    import truth as T
    lines = text.split("\n")
    rows = T.decode(smap)
    ilines = T.decode(inner[0]) if inner else None
    out, start = [], 0
    for li, row in enumerate(rows):
        line = lines[li] if li < len(lines) else ""
        ascii_line = line.isascii()
        pc, pb = 0, 0
        for col, src, ol, oc in row:
            if ascii_line:
                off = start + col
            else:
                if col >= pc:
                    pb += len(line[pc:col].encode())
                else:
                    pb = len(line[:col].encode())
                pc = col
                off = start + pb
            if ilines is not None:
                il = ol - inner[1]
                if il < 0 or il >= len(ilines) or not ilines[il]:
                    continue
                r = ilines[il]
                k = max(bisect.bisect_right(r, (oc, 1 << 62)) - 1, 0)
                src, ol, oc = r[k][1:]
            out.append((off, src, ol, oc))
        start += len(line.encode()) + 1
    return out


def origin_runs(segs, origins):
    """Per truth segment, its origin, run-length coded: (names, kinds, [[first segment index, name, kind]...]) with
    name/kind -1 for 'no origin' (the shipper's own code)."""
    names, kinds, runs = [], [], []
    ni, ki = {}, {}
    prev = None
    for k, (_off, src, ol, _oc) in enumerate(segs):
        o = origins.at(src, ol)
        if o != prev:
            if o is None:
                runs.append([k, -1, -1])
            else:
                kind, name = o
                if name not in ni:
                    ni[name] = len(names)
                    names.append(name)
                if kind not in ki:
                    ki[kind] = len(kinds)
                    kinds.append(kind)
                runs.append([k, ni[name], ki[kind]])
            prev = o
    return names, kinds, runs
