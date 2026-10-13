"""exp097 step 4: the versions Claude Code's bundle DECLARES for its unclued libraries (never a lockfile).

Reads the bundle and prints, per package, the declaration found (kind, byte offset, version) or that none exists.
Declarations read:
  - a kept license banner `/*! <name> v<version>` (MSAL ships one per file);
  - Stainless SDKs' version constant: the identifier sent as `"X-Stainless-Package-Version"` and used in the
    `${this.constructor.name}/JS ${VERSION}` user agent, assigned one string literal (@anthropic-ai/sdk).
Prints names, versions and offsets only (no code).
usage: python3 -I cc_declared.py <bundle> [--pins]   (--pins: only the name@version lines)
"""
import re
import sys

PACKAGES = ["@anthropic-ai/sdk", "@azure/msal-node", "@azure/msal-common", "@modelcontextprotocol/sdk",
            "@growthbook/growthbook", "ink"]


def banners(text):
    out = {}
    for m in re.finditer(r"/\*! (@?[a-z0-9][\w./-]*) v(\d+\.\d+\.\d+[\w.-]*)", text):
        out.setdefault(m.group(1), set()).add((m.group(2), m.start()))
    return out


def stainless(text):
    ids = set(re.findall(r'"X-Stainless-Package-Version":([A-Za-z_$][\w$]*)', text))
    found = []
    for ident in sorted(ids):
        if not re.search(r"\$\{this\.constructor\.name\}/JS \$\{" + re.escape(ident) + r"\}", text):
            continue
        for m in re.finditer(r"(?<![\w$.])" + re.escape(ident) + r'="(\d+\.\d+\.\d+[\w.-]*)"', text):
            found.append((m.group(1), m.start()))
    return found


def main():
    text = open(sys.argv[1], encoding="utf-8", errors="replace").read()
    b = banners(text)
    decl = {}
    for name in PACKAGES:
        if name in b:
            vs = sorted(b[name])
            decl[name] = ("banner", vs[0][1], sorted({v for v, _ in vs}))
    st = stainless(text)
    if st:
        # the Stainless user agent names the client class; Claude Code's is Anthropic's SDK
        decl["@anthropic-ai/sdk"] = ("stainless-version-constant", st[0][1], sorted({v for v, _ in st}))
    for name in PACKAGES:
        if name in decl:
            kind, at, versions = decl[name]
            if "--pins" in sys.argv:
                if len(versions) == 1:
                    print(f"{name}@{versions[0]}")
            else:
                print(f"{name}: {kind} at byte {at}: {', '.join(versions)}")
        elif "--pins" not in sys.argv:
            print(f"{name}: NO declared version in the bundle")


if __name__ == "__main__":
    main()
