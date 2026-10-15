"""exp115: the vendored-copy PATH GRAMMAR (lab twin of the Rust rule). A unit row's file names ANOTHER package when
its path (relative to the owner's package root) says the file is a copy of that package:

  nm      node_modules/<pkg>/...                      anywhere in the path
  vendor  <marker>/<pkg>/... or <marker>/<pkg>.js     marker = a directory named vendor, vendors, vendored,
                                                      third_party, 3rd_party, compiled, bundled, prebundled
                                                      (or ending in -vendor/_vendor/.vendor, e.g. lib-vendor)
  libs    <libs|deps|external|externals>/<pkg>...     weak marker: the name must be a KNOWN package
  stem    .../<pkg>[-HASH|.HASH].js                   the file's own stem (bundler chunk hash stripped) is a KNOWN
                                                      package

A name the owner itself answers to (its full name or its unscoped basename) or one of its family never counts."""
import re

STRONG = re.compile(r"^(?:.*[-_.])?(?:vendor|vendors|vendored|third[-_]?party|3rd[-_]?party|compiled|bundled|"
                    r"prebundled)$")
WEAK = {"libs", "deps", "external", "externals"}
EXT = re.compile(r"(?:\.(?:min|prod|production|development|dev|umd|esm|cjs|mjs|js|cjs|node|bundle|browser|global))+$")
HASH = re.compile(r"[-.]([A-Za-z0-9_-]{8}|[0-9a-f]{8,20})$")
GENERIC = {"index", "main", "src", "lib", "dist", "cjs", "esm", "es", "umd", "build", "utils", "util", "common",
           "shared", "types", "chunks", "chunk", "_chunks", "node", "browser", "module", "modules", "core"}


def stem(name):
    s = EXT.sub("", name)
    m = HASH.search(s)
    if m and (any(c.isdigit() for c in m.group(1)) or any(c.isupper() for c in m.group(1))):
        s = s[: m.start()]
    return s


def own_names(owner):
    out = {owner}
    if owner.startswith("@") and "/" in owner:
        out.add(owner.split("/", 1)[1])
    return out


def pkg_at(segs, i):
    """The package name starting at segs[i] (two segments when scoped); (name, is_dir)."""
    if i >= len(segs):
        return None, False
    s = segs[i]
    if s.startswith("@") and i + 1 < len(segs):
        nxt = segs[i + 1]
        is_dir = i + 1 < len(segs) - 1
        return s + "/" + (nxt if is_dir else stem(nxt)), is_dir
    is_dir = i < len(segs) - 1
    return (s if is_dir else stem(s)), is_dir


def named_package(owner, file, known, same_family=lambda a, b: False):
    """(name, kind) or None."""
    segs = [s for s in file.split("/") if s and s != "."]
    own = own_names(owner)

    def ok(n):
        return n and n not in own and n.lower() not in GENERIC and not same_family(owner, n)

    for i, s in enumerate(segs[:-1]):
        low = s.lower()
        if low == "node_modules":
            n, _ = pkg_at(segs, i + 1)
            if ok(n):
                return n, "nm"
        elif STRONG.match(low):
            n, is_dir = pkg_at(segs, i + 1)
            if ok(n) and (is_dir or n in known):
                return n, "vendor"
        elif low in WEAK:
            n, _ = pkg_at(segs, i + 1)
            if ok(n) and n in known:
                return n, "libs"
    if segs:
        n = stem(segs[-1])
        if ok(n) and n in known:
            return n, "stem"
    return None
