"""exp097 step 2: adopt the index-v2 builds under their recipe slice names, so the recipe-keyed builder reuses them.

Index v2's builder wrote `build/bun/<tag>` and `build/esbuild/<tag>` per install, stamped `recipe/4\n<entry text>`.
Those command lines ARE the recipes `bun:target=node,format=cjs,minify` and `esbuild:target=node,format=cjs,minify`
under recipe/4 (crates/humanify-index/src/recipe.rs at a537a92b and at this branch: same flags, same TLA->esm
retry, same externals retry), built by the same bundler binaries (bun 1.4.0, esbuild 0.28.2). This hard-links each
stamped build into `build/<slice>/<tag>` with the new stamp (a NEW file: the old one is never written), so the new
builder fingerprints them instead of re-running the bundlers. A build whose stamp does not match its entry (stale,
failed) is not adopted: the builder rebuilds it.

usage: python3 -I adopt_builds.py <work dir> <recipe version> [--dry-run]
"""
import os
import sys

PAIRS = [("bun", "bun-node-cjs-min", "bun:target=node,format=cjs,minify"),
         ("esbuild", "esbuild-node-cjs-min", "esbuild:target=node,format=cjs,minify")]
ENTRIES = [("req", "entries/req.cjs"), ("dyn", "entries/dyn.mjs")]


def main():
    work, recipe_version = sys.argv[1], sys.argv[2]
    dry = "--dry-run" in sys.argv
    counts = {"adopted": 0, "already": 0, "stale": 0, "absent": 0}
    for inst in sorted(os.listdir(work)):
        d = os.path.join(work, inst)
        for tag, entry in ENTRIES:
            try:
                text = open(os.path.join(d, entry), encoding="utf-8").read()
            except OSError:
                counts["absent"] += 1
                continue
            for old, slice_, spelling in PAIRS:
                src = os.path.join(d, "build", old, tag)
                dst = os.path.join(d, "build", slice_, tag)
                try:
                    stamp = open(os.path.join(src, ".recipe"), encoding="utf-8").read()
                except OSError:
                    counts["absent"] += 1
                    continue
                if stamp != f"{recipe_version}\n{text}":
                    counts["stale"] += 1
                    continue
                if os.path.exists(os.path.join(dst, ".recipe")):
                    counts["already"] += 1
                    continue
                counts["adopted"] += 1
                if dry:
                    continue
                os.makedirs(dst, exist_ok=True)
                for f in os.listdir(src):
                    if f != ".recipe" and os.path.isfile(os.path.join(src, f)):
                        t = os.path.join(dst, f)
                        if os.path.exists(t):
                            os.unlink(t)
                        os.link(os.path.join(src, f), t)
                with open(os.path.join(dst, ".recipe"), "w", encoding="utf-8") as fh:
                    fh.write(f"{recipe_version}\n{spelling}\n{text}")
    print(counts)


if __name__ == "__main__":
    main()
