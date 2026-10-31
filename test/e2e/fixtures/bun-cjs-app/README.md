# bun-cjs-app — a CommonJS app: its own modules in src/, its library in vendor/

A real, minified Bun 1.3.14 build (`bun build bin/cli.js --target=bun
--format=cjs --minify`, the bun-dev corpus's flags on Bun's wrapped
CommonJS layout) of a small app written in CommonJS, built for exp172
(`experiments/172-cjs-app-placement/README.md`).

## What it holds

- `bin/cli.js`, the published bin (no shebang: Bun would print it above
  its `// @bun @bun-cjs` line, how its loader is recognised): it only
  requires the app, so the app
  text outside the modules is `require_cli();`;
- the app's own CommonJS modules: `src/cli.js` → `src/run.js` →
  `src/format.js`, `src/steps/index.js` and 48 steps, each requiring
  `src/version.js`, which reads the app's version from its own
  `package.json` (a CLI printing its version);
- one library, `lib/tinylib/` (a local folder: `node_modules/` is
  gitignored), which declares itself by its `/*! tinylib v1.0.0 | MIT */`
  banner;
- a deterministic stdout report.

v1.1.0 bumps the version and changes the format module's separator.

## What it proves (exp172)

Bun wraps EVERY CommonJS module as `var X = __commonJS(…)`, app or
library, and the unpack used to write every one under `vendor/`: an app
written in CommonJS landed wholly in `vendor/`. Now
(`crates/humanify-core/src/modules/app_factories.rs`):

- the app text only runs `require_cli()`, so that factory is the app's
  ENTRY;
- the entry's modules require the app's own `package.json`, which names
  the app's OWN package (`cjs-cli-app`);
- every module that requires an app module is an app module, up to the
  entry; the library declares itself and requires nothing of the app.

The app's 54 modules stay in the app and the split groups them under
`src/`; `tinylib` is the only vendored module (`expect.placement`). Both
trees boot to the input's exact stdout. An app module nothing ties to the
app (no app module above it requires app code, no own-package evidence)
would stay in `vendor/`: the rule never moves anything DOWNWARD from the
app, because an unattributed module the app requires may be an unindexed
library.

## Regenerating

```bash
PATH="$HOME/.bun/bin:$PATH" bash test/e2e/fixtures/bun-cjs-app/build.sh
```

The script refuses any Bun but 1.3.14, builds each version from inside a
scratch copy of `source/v<ver>/` and prints the sha256s. Built twice,
identical (2026-10-31):

```
cb7c56644047dc6b9a67238cf3a1bb3587b7835eb62cd617762076c15d28cfad  build/v1.0.0/build/index.js
3b0a51994c93c41a07012b63b5755ae384d76a1a72583b598536b6d550fb9d31  build/v1.1.0/build/index.js
```
