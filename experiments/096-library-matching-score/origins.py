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


class Origins:
    """origin(source index, original line) -> (kind, package) or None, for one map's sources."""

    def __init__(self, paths, contents, shippers):
        self.whole, self.regions = {}, {}
        contents = contents or [None] * len(paths)
        for si, (p, c) in enumerate(zip(paths, contents)):
            ship = shippers[si]
            inside = p.rsplit("node_modules/", 1)[-1] if "node_modules/" in p else p
            m = VENDOR_DIR.search(inside)
            if m and m.group(1) != ship:
                self.whole[si] = ("path", m.group(1))
                continue
            if c:
                r = [(ln, o if o != ship else None) for ln, o in marker_regions(c)]
                if any(o for _, o in r):
                    self.regions[si] = ([x[0] for x in r], [x[1] for x in r])

    def at(self, si, oline):
        if si in self.whole:
            return self.whole[si]
        r = self.regions.get(si)
        if r is None:
            return None
        k = bisect.bisect_right(r[0], oline) - 1
        return ("marker", r[1][k]) if k >= 0 and r[1][k] is not None else None


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
