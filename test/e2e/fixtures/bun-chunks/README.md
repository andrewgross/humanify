# bun-chunks — a compiled, code-split Bun app

The `chunks` build of `../bun-apps` (recipe, source generator and sha256s
there): Bun 1.3.14, `bun build src/main.js --compile --splitting --format
esm --minify`, the executable unpacked with bun-decompile. The result is
Claude Code's layout from 2.1.242: `index.js` is an ESM entry under `// @bun`
that imports its siblings as `/$bunfs/root/<chunk>.js` — a runtime chunk
(Bun's interop helpers), one chunk per module loaded through `import()`
(archive, codec, ledger, router) and a chunk for the helper they share.
units and palette are imported statically and live in the entry.

## What it proves (exp216)

The input reader (`unpack::chunk_link`) links the chunks into one
`@bun-cjs` program before detection, and the run proceeds as for any Bun
bundle: `detect` says bun / the bun adapter, the ms and clock factories are
vendored, each module's report lands in its own file (`expect.apart`), and
the split trees boot with the input's behaviour. The input itself is run
by Node with a resolve hook mapping `/$bunfs/root/` beside the entry
(`node_run_args` in the e2e runner). v1.1.0 changes the palette (static)
and archive (lazy) reports and adds a function to the CommonJS dependency;
archive's chunk gets a new content-hash name, the others keep theirs.
