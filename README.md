# HumanifyJS
> Deobfuscate Javascript code using LLMs ("AI")

This tool uses large language modeles (like ChatGPT & llama) and other tools to
deobfuscate, unminify, transpile, decompile and unpack Javascript code. Note
that LLMs don't perform any structural changes – they only provide hints to
rename variables and functions. The heavy lifting is done by Babel on AST level
to ensure code stays 1-1 equivalent.

### Version 2 is out! 🎉

v2 highlights compared to v1:
* Python not required anymore!
* A lot of tests, the codebase is actually maintanable now
* Renewed CLI tool `humanify` installable via npm

### ➡️ Check out the [introduction blog post][blogpost] for in-depth explanation!

[blogpost]: https://thejunkland.com/blog/using-llms-to-reverse-javascript-minification

## Example

Given the following minified code:

```javascript
function a(e,t){var n=[];var r=e.length;var i=0;for(;i<r;i+=t){if(i+t<r){n.push(e.substring(i,i+t))}else{n.push(e.substring(i,r))}}return n}
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

Using `humanify local` is of course free, but may take more time, be less
accurate and not possible with your existing hardware.

## Getting started

### Installation

Prerequisites:
* Node.js >=20

The preferred whay to install the tool is via npm:

```shell
npm install -g humanifyjs
```

This installs the tool to your machine globally. After the installation is done,
you should be able to run the tool via:

```shell
humanify
```

If you want to try it out before installing, you can run it using `npx`:

```
npx humanifyjs
```

This will download the tool and run it locally. Note that all examples here
expect the tool to be installed globally, but they should work by replacing
`humanify` with `npx humanifyjs` as well.

### Usage

Next you'll need to decide whether to use `openai`, `gemini` or `local` mode. In a
nutshell:

* `openai` or `gemini` mode
  * Runs on someone else's computer that's specifically optimized for this kind
    of things
  * Costs money depending on the length of your code
  * Is more accurate
* `local` mode
  * Runs locally
  * Is free
  * Is less accurate
  * Runs as fast as your GPU does (it also runs on CPU, but may be very slow)

See instructions below for each option:

### OpenAI mode

You'll need a ChatGPT API key. You can get one by signing up at
https://openai.com/.

There are several ways to provide the API key to the tool:
```shell
humanify openai --apiKey="your-token" obfuscated-file.js
```

Alternatively you can also use an environment variable `OPENAI_API_KEY`. Use
`humanify --help` to see all available options.

### Gemini mode

You'll need a Google AI Studio key. You can get one by signing up at
https://aistudio.google.com/.

You need to provice the API key to the tool:

```shell
humanify gemini --apiKey="your-token" obfuscated-file.js
```

Alternatively you can also use an environment variable `GEMINI_API_KEY`. Use
`humanify --help` to see all available options.

### Local mode

The local mode uses a pre-trained language model to deobfuscate the code. The
model is not included in the repository due to its size, but you can download it
using the following command:

```shell
humanify download 2b
```

This downloads the `2b` model to your local machine. This is only needed to do
once. You can also choose to download other models depending on your local
resources. List the available models using `humanify download`.

After downloading the model, you can run the tool with:

```shell
humanify local obfuscated-file.js
```

This uses your local GPU to deobfuscate the code. If you don't have a GPU, the
tool will automatically fall back to CPU mode. Note that using a GPU speeds up
the process significantly.

Humanify has native support for Apple's M-series chips, and can fully utilize
the GPU capabilities of your Mac.

## Features

The main features of the tool are:
* Uses ChatGPT functions/local models to get smart suggestions to rename
  variable and function names
* Uses custom and off-the-shelf Babel plugins to perform AST-level unmanging
* Uses Webcrack to unbundle Webpack bundles

## Development

### Prerequisites

* Node.js >= 20
* npm

### Setup

```shell
git clone <repo-url>
cd humanify
npm install
npm run build
```

### Project Structure

```
src/
  analysis/          # AST analysis: function graphs, structural hashing, fingerprinting
  llm/               # LLM providers (OpenAI-compatible, Gemini, local llama), prompts, rate limiting
  rename/            # Rename processor: dependency-ordered function processing with LLM
  plugins/           # Babel plugins and pipeline orchestration (rename, prettier, webcrack)
  commands/          # CLI command handlers
test/
  e2e/
    fixtures/        # Real-world packages (mitt, zustand, nanoid) with fixture configs
    harness/         # E2E test harness: setup, minify, validate, humanify, debug
    snapshots/       # Baseline snapshots for fingerprint validation and humanify quality
    *.fptest.ts      # Fingerprint test files (node:test wrappers)
```

Unit tests are colocated next to their source files as `*.test.ts`.

### Test Suites

The project has several test suites at different levels:

#### Unit Tests

```shell
npm run test:unit
```

Fast, no external dependencies. Tests core logic: structural hashing, fingerprinting, function graph building, LLM prompt generation, name validation, rate limiting, rename processing.

#### E2E Tests (built-in)

```shell
npm run test:e2e
```

Builds the project and runs `*.e2etest.ts` files. Tests the CLI and rename pipeline with mock providers. No LLM required.

#### Fingerprint Tests

```shell
npm run test:fingerprint
```

Runs `*.fptest.ts` files under `test/e2e/`. These are node:test wrappers around the E2E validation harness that verify fingerprint matching accuracy against stored snapshots. Requires fixtures to be set up first (see below).

Quick single-fixture run:

```shell
npm run test:fingerprint:quick   # runs mitt only
```

#### E2E Validation Harness

The harness under `test/e2e/harness/` is a standalone CLI for working with real-world fixture packages. It has several commands:

**List available fixtures:**

```shell
npm run e2e -- list
```

**Set up a fixture** (clones the repo, checks out versions, builds):

```shell
npm run e2e -- setup mitt
npm run e2e -- setup zustand
npm run e2e -- setup nanoid
```

**Validate fingerprint matching** across version pairs:

```shell
npm run e2e -- validate mitt                     # all version pairs, default minifier
npm run e2e -- validate mitt 3.0.0 3.0.1         # specific version pair
npm run e2e -- validate mitt --all-minifiers     # terser + esbuild + swc
npm run e2e -- validate mitt --update-snapshot   # save baseline
npm run e2e -- validate mitt --ci                # compare against baseline, fail on drift
npm run e2e -- validate mitt --verbose           # show detailed failure output
npm run e2e -- validate mitt --show-diff         # show source diff between versions
```

**Debug a specific function:**

```shell
npm run e2e -- debug mitt 3.0.0 3.0.1 --function emit
```

**Run the LLM humanify pipeline** (requires an OpenAI-compatible LLM endpoint):

```shell
HUMANIFY_TEST_BASE_URL=http://localhost:8080/v1 \
HUMANIFY_TEST_MODEL=your-model-name \
HUMANIFY_TEST_API_KEY=your-key \
npm run e2e -- humanify mitt 3.0.0

# Or use the shorthand script:
npm run e2e:humanify -- mitt 3.0.0
```

Options: `-v` (show renamed output), `-vv` (full debug with LLM traces), `--update-snapshot`, `--ci`, `--minifier <id>`, `--all-minifiers`.

### How Fixtures Work

Each fixture in `test/e2e/fixtures/<name>/` has a `fixture.config.json` that defines:

- **package**: npm package name
- **repo**: git URL to clone
- **sourceStrategy**: how to check out versions (git tags or commit SHAs)
- **entryPoints**: source files to process
- **buildCommand**: optional TypeScript compilation step
- **versionPairs**: pairs of versions to compare, with optional override rules

When you run `setup`, the harness clones the repo, checks out each version, copies entry points, and builds them. The built JS files are then available for minification and analysis.

### How Fingerprint Validation Works

The `validate` command tests that the structural fingerprinting system correctly identifies functions across minified versions:

1. **Minify** both versions using the selected minifier(s) (terser, esbuild, swc)
2. **Build ground truth** by parsing the original source and matching functions by name/hash across versions
3. **Compute fingerprints** from the minified code (structural hashes that are position-independent)
4. **Match functions** across minified versions using fingerprints
5. **Link back** to source via source maps to verify correctness
6. **Validate** that unchanged functions match, modified functions differ, and no false positives occur

Results are compared against stored snapshots. Key metrics:

- **Cache reuse accuracy**: do unchanged functions get matching fingerprints?
- **Change detection accuracy**: do modified functions get different fingerprints?
- **Overall accuracy**: combined score

### How Humanify Validation Works

The `humanify` command tests the full LLM rename pipeline on real minified code:

1. **Minify** a fixture version
2. **Run the rename pipeline** through `createRenamePlugin()` with a real LLM provider
3. **Validate** the output: must parse as valid JS with the same function count
4. **Measure quality**: identifiers renamed, average name length, name recovery score (fuzzy match against source ground truth)

The snapshot captures metrics and an output hash (NOT the full output, which is LLM-nondeterministic). CI mode checks that metrics don't regress significantly.

Requires environment variables:

| Variable | Required | Description |
|----------|----------|-------------|
| `HUMANIFY_TEST_BASE_URL` | Yes | OpenAI-compatible API endpoint |
| `HUMANIFY_TEST_MODEL` | Yes | Model identifier |
| `HUMANIFY_TEST_API_KEY` | No | API key (defaults to "dummy" for local servers) |

### Snapshots

There are two types of snapshots:

**Fingerprint snapshots** (`test/e2e/snapshots/<fixture>/`): track fingerprint matching accuracy. These are deterministic and should always match exactly.

**Humanify snapshots** (`test/e2e/snapshots/humanify/<fixture>/`): track LLM output quality metrics. These allow some drift since LLM output is nondeterministic, but flag significant regressions.

To update snapshots after intentional changes:

```shell
npm run e2e -- validate mitt --update-snapshot
npm run e2e -- humanify mitt 3.0.0 --update-snapshot
```

### Available Minifiers

All validation and humanify commands support `--minifier <id>` or `--all-minifiers`:

| ID | Tool | Notes |
|----|------|-------|
| `terser-default` | Terser | Default. Most common production minifier |
| `esbuild-default` | esbuild | Fastest, different mangling strategy |
| `swc-default` | SWC | Rust-based, different optimization patterns |

Testing across minifiers verifies that fingerprinting and renaming work regardless of which tool produced the minified code.

## Contributing

If you'd like to contribute, please fork the repository and use a feature
branch. Pull requests are warmly welcome.

## Licensing

The code in this project is licensed under MIT license.
