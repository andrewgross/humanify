# bun-chunk-assets — a compiled, code-split Bun app with embedded files

The `assets` app of `../bun-apps/source/gen.mjs` (the bun-chunks app plus a
fifth lazy module, `notes`), built by `build.sh` with **Bun 1.4.1**
(`bun build src/main.js --compile --splitting --format esm --minify`) and
unpacked with bun-decompile. `notes` imports three text files
(`with { type: "text" }`: `intro.md`, not ASCII, so Bun stores it as
UTF-16; `rules.txt`, with backticks, `${`, a backslash and CRLF;
`page.html`) and one `file` asset (`blob.bin`), and prints all four.

Bun 1.4.1 writes a text import inside an executable as a separate embedded
file read at run time (`var t=e("/$bunfs/root/intro-<hash>.md")`, `e` the
runtime chunk's alias of `import.meta.require`) and a `file` import as the
path string `"/$bunfs/root/blob-<hash>.bin"` — Claude Code's shapes from
2.1.246 (exp217).

## What it proves (exp217)

The linker writes each text read as the text and points the path at the
output tree's copy (`embedded/blob-<hash>.bin`, written by the finish
stage), and the split trees print exactly what the compiled executable
printed. Node cannot run the unpacked input itself (it has no
`import.meta.require`), so `fixture.config.json` sets
`"recordedBehavior": true`: the input's behaviour is
`build/v<ver>/compiled.stdout`, recorded by `build.sh` from the
executable. v1.1.0 changes the intro (a new content-hash name) and the
archive report.

## Regenerating

```bash
BUN=/path/to/bun-1.4.1 BUN_DECOMPILE=/path/to/bun-decompile \
  bash test/e2e/fixtures/bun-chunk-assets/build.sh
```

Built twice, identical sha256s (2026-11-12):

```
443a1b03307c30b75f756e5c4eccf1bd66fd7922aecda1c36029f5317700dec9  build/v1.0.0/build/archive-3efnd10p.js
ad6170c304e1b0a6cfe3b1ba6b40c065f646428f623f664ad31d54b98735195c  build/v1.0.0/build/blob-40amdv8m.bin
8c42eb6f39d11b4460d4ded408d2fea6ccdd7b8002f34f940fe6a6a100a6eaf0  build/v1.0.0/build/codec-kak2szwr.js
f923e318368e3429a69f03f6d77eb8462766642fc9b194ba4cba035c67c5f7b7  build/v1.0.0/build/index.js
0dc2381ca9f1208be9087a87783c11d528bf39b9c8c5281eb665458122ef01e5  build/v1.0.0/build/intro-ryxyrahr.md
768477db19014963492b3ee285da5715f4df535965cc60cc65733dcff1dbab25  build/v1.0.0/build/ledger-geg9j7n5.js
37f586f9467b1edf7f9e2ae12a7ac69b79c909ce7c5b0fd6a6955214da7fa02d  build/v1.0.0/build/main-tbbj69a9.js
4c9289b0f7e93920cecd8eff298173c8f692cad545c8d557812b368525257445  build/v1.0.0/build/main-zj6tg49h.js
77dde514ad84e7711f3b295a41ce01cbed288f3d7c8f9d810fe958dcd68ec9a1  build/v1.0.0/build/notes-aeyemmy3.js
0f3603e1c2d4a3f8d80c353e0498a99a4b0c4a35aaf9bd9e60ea88db51361676  build/v1.0.0/build/page-xrggf6wd.html
19a4e74cac65501d9bee67bb4903f23ae87fa928c49355ee668cfb4123b48eb4  build/v1.0.0/build/router-xc0b09y7.js
92f1f7ae3588fab6ccdf09a0a355dd1bedbbaf7933eca7d3d259d944afe113a7  build/v1.0.0/build/rules-mntrhk0m.txt
53776d1bb4d3d21cc8177169454f8953e57013c93df9b393659d371755f081b6  build/v1.0.0/compiled.stdout
e84571a6b2f8fd7a7c2cd93cd0df142739d20badf60b922294ed9c9f75b17762  build/v1.1.0/build/archive-jdq32tyc.js
ad6170c304e1b0a6cfe3b1ba6b40c065f646428f623f664ad31d54b98735195c  build/v1.1.0/build/blob-40amdv8m.bin
8c42eb6f39d11b4460d4ded408d2fea6ccdd7b8002f34f940fe6a6a100a6eaf0  build/v1.1.0/build/codec-kak2szwr.js
7a0d04932382ba003cd273ef8b60ca8d68157ab5476a8645fce45bc512bdd560  build/v1.1.0/build/index.js
00d9f701d759cc0304507ae7b2780c88680ee8a17eb02d6124bf37964b6c1135  build/v1.1.0/build/intro-gfkgamy4.md
768477db19014963492b3ee285da5715f4df535965cc60cc65733dcff1dbab25  build/v1.1.0/build/ledger-geg9j7n5.js
37f586f9467b1edf7f9e2ae12a7ac69b79c909ce7c5b0fd6a6955214da7fa02d  build/v1.1.0/build/main-tbbj69a9.js
4c9289b0f7e93920cecd8eff298173c8f692cad545c8d557812b368525257445  build/v1.1.0/build/main-zj6tg49h.js
c3f9df6665dde7af2d2f860921f13ef915ca726e0a758b2277694149e5d234bd  build/v1.1.0/build/notes-38vjp8ze.js
0f3603e1c2d4a3f8d80c353e0498a99a4b0c4a35aaf9bd9e60ea88db51361676  build/v1.1.0/build/page-xrggf6wd.html
19a4e74cac65501d9bee67bb4903f23ae87fa928c49355ee668cfb4123b48eb4  build/v1.1.0/build/router-xc0b09y7.js
92f1f7ae3588fab6ccdf09a0a355dd1bedbbaf7933eca7d3d259d944afe113a7  build/v1.1.0/build/rules-mntrhk0m.txt
e772ea3e750bfee636a5c212ebe40c808841c131b25867eff99635a52c063a6a  build/v1.1.0/compiled.stdout
```

`build/` sits under `.gitignore`'s `*/build/` pattern and is FORCE-ADDED,
like the other fixtures' builds.
