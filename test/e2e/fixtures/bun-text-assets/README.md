# bun-text-assets — app text modules and a vendored module that changes length

A real, unminified Bun 1.3.14 build (`bun build --target=bun --format=cjs`,
Claude Code's own layout) of a small app, built for finding #90
(`docs/rust-port/16-findings-queue.md`).

## What it holds

- two of the app's own TEXT files, required by app code:
  `src/prompts/system.cjs` (`module.exports = "## System prompt…"`) and
  `src/prompts/review.cjs` (a template literal, `# Code review checklist…`);
- a vendored CommonJS codec, `src/vendor/yamlish.cjs`, whose error message
  is LONGER in v1.1.0 (`expected a YAML document string` → `… as a
string`) — so its structural hash, which keeps string lengths, moves;
- a vendored template helper, `src/vendor/template.cjs`, that requires its
  OWN text module (`template-text.cjs`);
- 60 small app functions (`src/steps.js`), so the bundle clears the
  split's 50-binding bundled-app gate with app code of its own, and a
  deterministic stdout report.

v1.1.0 also lengthens the system prompt's body (its heading is unchanged).

## What it proves (finding #90)

- **App text is an app asset.** Bun wraps every required file as a
  CommonJS module, so the app's two text files used to be vendored and sent
  to the package namer, which named prose after npm packages. Now a module
  whose whole body is `module.exports = <text>` and that only app code
  requires is written to `src/_assets/`, named from its first line:
  `src/_assets/system-prompt.js`, `src/_assets/code-review-checklist.js`
  (`expect.assets`). The template helper's text is required by a vendored
  module, so it stays in `vendor/`.
- **A vendor module whose string changed length keeps its file.** The
  exact structural-hash carry misses `yamlish` in v1.1.0; the content carry
  pairs it with v1.0.0's module and keeps its name, file and identifier
  (`expect.stays`: the vendor file holding `expected a YAML document` has
  the same path in both trees). On main the stub's declined vendor answer
  left it named by its new hash, so the file moved.

## Regenerating

```bash
PATH="$HOME/.bun/bin:$PATH" bash test/e2e/fixtures/bun-text-assets/build.sh
```

The script refuses any Bun but 1.3.14, builds each version from inside a
scratch copy of `source/v<ver>/` (no npm dependencies) and prints the
sha256s. Built twice, identical (2026-10-06):

```
d183977f7bda14e05196558a0e1b006d347449df4db1a2a0f078786af60c6ae8  build/v1.0.0/build/index.js
0392049fef57de8a9a0942d975265ac03d38579fca5b5461ecfc0724754da658  build/v1.1.0/build/index.js
```
