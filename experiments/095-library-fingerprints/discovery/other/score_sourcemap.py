"""exp095 discovery on an esbuild bundle that ships its source map (wrangler): every statement unit labelled by the
original source its bytes map to (a `node_modules/<pkg>` path = library <pkg>, anything else = first-party app code),
then the core report scored exactly: claims, library class and queue, by bytes.

usage: python3 -I score_sourcemap.py <bundle.js> <bundle.js.map> <units.tsv> <core-report.json> [<inner.map> <line-offset>]
The optional inner map composes a second hop: the bundle was minified from a file whose own map is <inner.map>, offset
by <line-offset> lines (a wrapper line added before minifying)."""
import bisect
import collections
import json
import sys

B64 = {c: i for i, c in enumerate('ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/')}


def vlq(seg):
    out, val, shift = [], 0, 0
    for ch in seg:
        d = B64[ch]
        val += (d & 31) << shift
        if d & 32:
            shift += 5
        else:
            out.append(-(val >> 1) if val & 1 else val >> 1)
            val, shift = 0, 0
    return out


def package_of(source):
    if 'node_modules/' not in source:
        return None
    rest = source.rsplit('node_modules/', 1)[1].split('/')
    return '/'.join(rest[:2]) if rest[0].startswith('@') else rest[0]


def decode_lines(smap):
    """Per generated line: sorted (column, source index, original line, original column)."""
    out = []
    src = oline = ocol = 0
    for group in smap['mappings'].split(';'):
        col = 0
        row = []
        for seg in filter(None, group.split(',')):
            v = vlq(seg)
            col += v[0]
            if len(v) >= 4:
                src += v[1]
                oline += v[2]
                ocol += v[3]
                row.append((col, src, oline, ocol))
        out.append(row)
    return out


def compose(outer, inner, offset):
    """outer's mappings with each original position looked up in `inner` (one hop back): a new map dict."""
    lines = decode_lines(inner)
    def at(line, col):
        line -= offset
        if line < 0 or line >= len(lines) or not lines[line]:
            return None
        row = lines[line]
        k = bisect.bisect_right(row, (col, 1 << 62)) - 1
        return row[max(k, 0)][1]
    return {'sources': inner['sources'], 'resolved': [[(c, at(ol, oc)) for c, _, ol, oc in row] for row in decode_lines(outer)]}


def segments(text, smap):
    """(byte offset, source index) for every mapping segment, in order."""
    line_starts = [0]
    lines = text.split('\n')
    for ln in lines[:-1]:
        line_starts.append(line_starts[-1] + len(ln.encode()) + 1)
    out = []
    if 'resolved' in smap:
        for li, row in enumerate(smap['resolved']):
            line = lines[li] if li < len(lines) else ''
            for col, src in row:
                if src is not None:
                    out.append((line_starts[li] + len(line[:col].encode()), src))
        return out
    src = 0
    for li, group in enumerate(smap['mappings'].split(';')):
        col = 0
        line = lines[li] if li < len(lines) else ''
        for seg in filter(None, group.split(',')):
            v = vlq(seg)
            col += v[0]
            if len(v) >= 4:
                src += v[1]
                out.append((line_starts[li] + len(line[:col].encode()), src))
    return out


def main():
    text = open(sys.argv[1], encoding='utf8').read()
    smap = json.load(open(sys.argv[2]))
    if len(sys.argv) > 6:
        smap = compose(smap, json.load(open(sys.argv[5])), int(sys.argv[6]))
    sources = smap['sources']
    segs = segments(text, smap)
    offs = [s[0] for s in segs]
    units = []
    for l in open(sys.argv[3]):
        s, e, owned, kind, runtime = l.rstrip('\n').split('\t')
        units.append((int(s), int(e), int(owned), kind, runtime == 'true'))
    report = json.load(open(sys.argv[4]))

    def label(s, e):
        """The package most of [s, e)'s mapped bytes come from (None = app), by segment bytes."""
        k = max(bisect.bisect_right(offs, s) - 1, 0)
        votes = collections.Counter()
        while k < len(segs) and segs[k][0] < e:
            a = max(segs[k][0], s)
            b = min(segs[k + 1][0] if k + 1 < len(segs) else e, e)
            if b > a:
                votes[package_of(sources[segs[k][1]]) or '<app>'] += b - a
            k += 1
        return votes.most_common(1)[0][0] if votes else '<unmapped>'

    stmt = [(s, e, o) for s, e, o, kind, rt in units if kind in ('Statement',) or kind.startswith('LazyBody')
            if not rt]
    lab = {(s, e): label(s, e) for s, e, _ in stmt}
    tot = collections.Counter()
    for s, e, o in stmt:
        tot['lib' if lab[(s, e)] not in ('<app>', '<unmapped>') else lab[(s, e)]] += o
    print('statement bytes by label:', dict(tot))
    owned = {(s, e): o for s, e, o in stmt}

    def side(rows, pkg_key=None):
        c = collections.Counter()
        wrong = []
        app_rows = []
        for r in rows:
            k = (r['start'], r['end'])
            if k not in lab:
                continue
            t = lab[k]
            kind = 'lib' if t not in ('<app>', '<unmapped>') else t
            c[kind] += owned[k]
            if kind == 'lib' and pkg_key and r[pkg_key] != t:
                wrong.append((r[pkg_key], t, owned[k]))
            if kind == '<app>':
                app_rows.append((r.get(pkg_key or 'package'), owned[k], text[r['start']:r['start'] + 80]))
        return c, wrong, app_rows
    for name, rows, key in (('claims', report['claims'], 'package'), ('libraryClass', report['libraryClass'], 'package'),
                            ('queue', report['queue'], None)):
        c, wrong, app_rows = side(rows, key)
        print(f'{name}: units {len(rows)} bytes {dict(c)}; lib recall {c["lib"] / max(tot["lib"], 1):.4f}')
        if wrong:
            exact = sum(1 for w in wrong)
            print(f'   claimed package differs from the map on {exact} lib units:', collections.Counter((w[0], w[1]) for w in wrong).most_common(8))
        for a in app_rows[:10]:
            print('   APP:', a)
    seeds = [r for r in report['claims'] if r['rule'] == 'seed']
    c, _, _ = side(seeds, 'package')
    print('seeds bytes', dict(c))
    top = collections.Counter()
    for (s, e), t in lab.items():
        if t not in ('<app>', '<unmapped>'):
            top[t] += owned[(s, e)]
    print('library packages in statements (top):', top.most_common(15))


if __name__ == '__main__':
    main()
