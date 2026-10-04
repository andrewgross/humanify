# esbuild-kept-factory — a bundled module the unpack keeps in the app

The two committed builds of [`esbuild-bundle`](../esbuild-bundle/README.md)
(esbuild 0.27.2) with three HAND-EDITED lines each — not an esbuild output:

1. `var appCounter = 0;` at the top of the iife (an app binding);
2. `appCounter = appCounter + 1;` inside the `__commonJS` factory
   (`require_cjs_dep`), so the factory WRITES an app binding;
3. `` `appCounter: ${appCounter}`, `` in the report, so the boot step sees
   the write.

A vendor file cannot write a binding of the app's scope, so the unpack
keeps this factory in the app (finding #51's rule). Which code is a bundled
module is the unpack's decision, made once: the factory it kept is APP
code, and the naming stage names its inner functions in the main waves
with call-graph context (finding #80). Before #80 the naming stage re-ran
the module grammar on every file, found this factory again on esbuild's
formatted text, and pulled its five inner functions out of the waves —
the coverage sweep then asked for them one at a time.

Regenerate from `esbuild-bundle`'s builds with the three edits above
(`var __defProp` line, `var PREFIX = "cjs-dep";` line, `repeatChar` report
line).
