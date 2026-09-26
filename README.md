# HumanifyJS

> Deobfuscate Javascript code using LLMs ("AI")

This tool uses large language modeles (like ChatGPT & llama) and other tools to
deobfuscate, unminify, transpile, decompile and unpack Javascript code. Note
that LLMs don't perform any structural changes – they only provide hints to
rename variables and functions. The heavy lifting is done on the AST (a native
Rust pipeline built on oxc) to ensure code stays 1-1 equivalent.

### Version 2 is out! 🎉

v2 highlights compared to v1:

- Python not required anymore!
- A lot of tests, the codebase is actually maintanable now
- Renewed CLI tool `humanify` installable via npm

### ➡️ Check out the [introduction blog post][blogpost] for in-depth explanation!

[blogpost]: https://thejunkland.com/blog/using-llms-to-reverse-javascript-minification

## Example

Given the following minified code:

```javascript
function a(e, t) {
  var n = [];
  var r = e.length;
  var i = 0;
  for (; i < r; i += t) {
    if (i + t < r) {
      n.push(e.substring(i, i + t));
    } else {
      n.push(e.substring(i, r));
    }
  }
  return n;
}
```

The tool will output a human-readable version:

```javascript
function splitString(inputString, chunkSize) {
  var chunks = [];
  var stringLength = inputString.length;
  var startIndex = 0;
  for (; startIndex < stringLength; startIndex += chunkSize) {
    if (startIndex + chunkSize < stringLength) {
      chunks.push(inputString.substring(startIndex, startIndex + chunkSize));
    } else {
      chunks.push(inputString.substring(startIndex, stringLength));
    }
  }
  return chunks;
}
```

🚨 **NOTE:** 🚨

Large files may take some time to process and use a lot of tokens if you use
ChatGPT. For a rough estimate, the tool takes about 2 tokens per character to
process a file:

```shell
echo "$((2 * $(wc -c < yourscript.min.js)))"
```

So for refrence: a minified `bootstrap.min.js` would take about $0.5 to
un-minify using ChatGPT.

A local model behind an OpenAI-compatible server is of course free, but may take
more time and be less accurate.

## Getting started

The pipeline is a single native binary, `humanify`, built from the Rust
workspace in `crates/`. (Up to tag `m4` it was a TypeScript program; it was
ported, proven output-equivalent on the eval corpus, and replaced — see
`docs/rust-port/19-cutover.md`.)

### Build

Prerequisites:

- Rust, via [rustup](https://rustup.rs). The toolchain is pinned in
  `rust-toolchain.toml`; the first `cargo` call inside the repo installs it.
- Node.js >= 20 and npm — only for webpack/browserify inputs (the unpacker
  shells out to `scripts/webcrack-shim.ts`) and for the measurement harness.
  Bun-compiled and plain bundles need no Node.

```shell
git clone <repo-url>
cd humanify
cargo build --release --locked -p humanify-cli
npm ci --ignore-scripts        # only for webpack/browserify inputs and the harness
```

The binary is `target/release/humanify`. Copy it anywhere on your `PATH`, or
`cargo install --path crates/humanify-cli --locked`.

### Usage

Humanify talks to any OpenAI-compatible chat-completions endpoint: OpenAI
itself, or a local server (llama.cpp, vLLM, Ollama's OpenAI endpoint, …).

```shell
# OpenAI
humanify obfuscated.js -o out --model gpt-4o-mini --api-key "$OPENAI_API_KEY"

# A local OpenAI-compatible server
humanify obfuscated.js -o out --endpoint http://localhost:8000/v1 \
  --model openai/gpt-oss-20b --api-key local --reasoning-effort low
```

The API key comes from `--api-key`, else `HUMANIFY_API_KEY`, else
`OPENAI_API_KEY`.

Common options (`humanify --help` lists them all):

| option                       | what it does                                                                                                |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `-o, --output-dir <dir>`     | where the result goes (default `output`)                                                                    |
| `--split`                    | write a multi-file tree (`src/` + `vendor/` + a runnable CommonJS scaffold) instead of one file             |
| `--prior-version <file>`     | the previous release's `.humanify/humanified.js`: names, file layout and vendor names carry across versions |
| `--llm-cache <dir>`          | cache model answers on disk, keyed by request content — reruns are nearly free and deterministic            |
| `-c, --concurrency <n>`      | concurrent model requests                                                                                   |
| `--reasoning-effort <level>` | `low` / `medium` / `high`, for reasoning models only                                                        |
| `--stats-json <file>`        | a compact match/rename breakdown                                                                            |

Cross-version use: humanify release N, then humanify release N+1 with
`--prior-version <N's output>/.humanify/humanified.js`, and the diff between
the two trees is (as close as it can make it) only the real code change.

## Features

- An LLM proposes names for every binding; it never rewrites code. Renames are
  applied and validated on the AST, so the output stays 1-1 equivalent.
- Cross-version matching (structural fingerprints, statement twins) reuses
  the prior release's names for unchanged code, so successive releases diff
  cleanly.
- Unpacks Bun-compiled bundles natively; webpack/browserify through webcrack.
- Detects library code and keeps it out of the model's way.
- Splits a bundle into a stable, runnable multi-file tree.

## Development

### Layout

```
crates/
  humanify-cli/      # the binary: CLI surface, pipeline driver, reports
  humanify-core/     # the pipeline stages: detect, unpack, format, graph,
                     #   matching, naming, split/emit, finish
  humanify-llm/      # the OpenAI-compatible client and the response cache
  humanify-model/    # shared data types (ledgers, dumps, stats)
  humanify-parity/   # the dump differ (`--dump-artifacts` A vs B)
scripts/             # the gate (check.ts), the eval dispatcher (eval.ts),
                     #   the e2e stage, the webcrack shim
experiments/         # the measurement harness (lib/, 034-eval-harness/) and
                     #   the experiment records
test/                # harness tests, the e2e fixtures, test/parity goldens
docs/                # architecture, measurement rules, the Rust-port record
```

### The gate

One command runs every check, and prints which ran:

```shell
npm run check
```

It covers the Rust pipeline (fmt, clippy, unit, the release build, the
formatter's frozen goldens, the differ self-test, an end-to-end run of the
binary on the committed fixtures with a boot check) and the TypeScript
measurement harness (typecheck, lint, knip, unit). `CLAUDE.md` lists the
stages.

### Measuring a change

`npm run eval` is the cross-version eval: it builds the binary, humanifies
four real release pairs cold, and scores the diff between them as real change
vs. reducible noise. Read `CLAUDE.md` ("Validating cross-version changes")
and `docs/measurement-pitfalls.md` before trusting a number from it.

## Contributing

If you'd like to contribute, please fork the repository and use a feature
branch. Pull requests are warmly welcome.

## Licensing

The code in this project is licensed under MIT license.
