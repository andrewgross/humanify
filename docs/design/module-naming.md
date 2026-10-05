# Naming each module from its contents — design and sizing

STATUS: design + measurement only (2026-10-05). Nothing below is built. The
prompt was prototyped against the real server; no pipeline code changed.

Andrew, 2026-10-05: keep the pipeline general, not Claude-Code-specific.
Instead of naming files and lazy-init wrappers by a fixed rule (the file's
first function, or the one variable the wrapper sets up), "assess a better
way to name files using an LLM initially based on some contents or some of
the functional code in there as opposed to the first reference." The
slash-command rule from `docs/design/lazy-init-naming.md` is dropped.

This document replaces that design's recommendation (derive the wrapper
name from the first function). Its measurements of the wrappers still stand
and are referenced below.

Evidence: `/work/module-naming-2026-10-05/` — `build.py` (one record per
module, the evidence the prompt shows), `sample.py` (the 60-module sample,
`sample60.json`), `ask.py` (the prompt and the request), `sizing.py`,
`verdicts.txt` (the reading verdicts), `table60.md`, the raw answers
(`answers-*.json`) and every prompt and reply (`log-*.json`). Trees:
`candidate-5d4b2d9a-scratch` — the 2.1.216 scored tree and its 2.1.215
scratch base.

## The idea in one paragraph

Bun and esbuild keep a record of every original source file: one lazy-init
wrapper per file (`var initFoo = __esm(() => { … })`). Every split file
holds exactly one of these modules. So a module needs ONE name, and both the
file (`foo-bar.js`) and its wrapper (`initFooBar`) should say it. Today they
are named separately and by different means: the file after its first
function (`module_stem`), the wrapper by the model one identifier at a time
— and ~20% of wrapper names come back as filler (`initializeApplication29`).
The proposal: one step asks the model "what is this file for?", showing the
file's declarations, setup code, imports and a code excerpt, several files
per call. Its answer names both. It runs once per NEW module — every module
on a fresh run, only the unmatched ones on a later version. Matched modules
keep their path and wrapper name exactly as today. Nothing in it is specific
to Claude Code; it works for any bundle whose toolchain knows the module
boundaries.

## The prompt

### What it shows per module

Built from what the pipeline already has when the naming waves end (the
names below are the current names; nothing comes from the file name):

| line                                                     | from                                                                                                                           |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `Declares:`                                              | the module's top-level declarations, functions with their parameters, short constants                                          |
| `Its setup code assigns`                                 | the bindings the wrapper body assigns                                                                                          |
| `Other code reads this whole module as one object named` | the module object, when other code does `(initFoo(), fooNamespace)`                                                            |
| `Imports modules that declare`                           | for each module it loads, the first three names that module declares                                                           |
| `Uses libraries`                                         | the vendor files it requires (named before naming, by library detection)                                                       |
| `Strings`                                                | up to 8 distinct string literals                                                                                               |
| `Code`                                                   | the wrapper body first (what the initializer does), then every other statement's first lines, deepened until ~1,600 characters |

The module's own wrapper is shown as `MODULE_INIT`, the helper as `__esm`,
and other modules' wrappers never appear by name: in the proposed order
they are not named yet, and showing the model's old answer would just
invite an echo.

### System prompt (as prototyped)

```text
You name the source files of a decompiled JavaScript program, the way an
experienced engineer would lay out a real repository.
Each entry is ONE original source file (a module). You see what it declares
(names as they are now), what its setup code assigns, which other modules it
imports, the libraries it uses, its string literals, and an excerpt of its
code. Decide what the file is FOR and name that.
Rules:
- One kebab-case name per entry, 1-4 words, no extension, no path. It becomes
  the file's name AND its initializer's name (init + the name), so it must
  describe the whole file.
- Name the file's main responsibility: the thing its other declarations
  serve. A small helper that happens to come first is not the file's name.
- A file holding one main function may be named after what that function
  does (parse-retry-after); otherwise prefer a noun phrase (secret-redaction,
  oauth-flow, token-bucket).
- Describe what the file PROVIDES, not what it imports.
- Avoid generic words (utils, helpers, core, common, misc, index, module,
  init, setup, app, main) and counter numbers (initializer17).
- Every entry must get a DISTINCT name.
Reply with JSON only.
```

### One entry, as sent (module 7 of the sample)

````text
### m2
Declares: var cryptoOperation; var validateSymmetricKey; var validateAsymmetricKey; var determineKeyValidation
Its setup code assigns: cryptoOperation
Imports modules that declare: createCryptoKeyError, isAlgorithmMatch, getHashBits
Strings: "Uint8Array", "secret", " or ", "sign", "public", "private", "decrypt", "verify"
Code:
```js
var MODULE_INIT = __esm(() => {
    cryptoOperation = determineKeyValidation;
  });
var cryptoOperation;
var validateSymmetricKey = (algorithmName, key) => {
    if (key instanceof Uint8Array) {
  …
```
````

The prompt ends `Reply with JSON {"m1": "<file-name>", …} — one specific,
distinct name per file.` Keys are neutral (`m1`…`m6`), not the mechanical
stem: showing the first function's name as the key would anchor the answer
on exactly the thing we are trying to get away from.

The request is the pipeline's own shape: system + user message,
`response_format: json_object`, temperature 0, `max_tokens` 6000,
`reasoning_effort: low`, model `openai/gpt-oss-20b` on :8000.

### One owner, not a parallel namer

The existing file namer (`place::assign::namer`) already has every piece
this needs: the request type (`SplitNameRequest`, which even has an unused
`evidence` field), batching under the model's context budget
(`split_namer_batches`), dispatch and logging (`ProviderSplitNamer`), and the
answer check (`place::stems::accept_proposed_name`). The step should be a
new kind on that namer (`NameKind::Module`), whose entry renders the lines
above, with neutral keys — not a second namer. Two things in it change:

- its system prompt says "a decompiled JavaScript **CLI tool**"; the module
  kind (and, by Andrew's direction, the existing prompt too) should say
  "program";
- its echo rule (an answer equal to the mechanical stem counts as "no
  answer") is harmless here: "no answer" falls back to the mechanical stem,
  which is the same name. 7 of 60 answers equalled today's file name, all
  correct (e.g. `auto-mode-scan-task`, `ordered-map`).

## The sample

60 modules from the 2.1.216 scored tree, drawn by kind (seed 5), each one
also present UNCHANGED in the 2.1.215 base (same statements, same file) so
the base could be asked the same question:

| kind              | taken | what it means                                                           |
| ----------------- | ----: | ----------------------------------------------------------------------- |
| helper-first      |    10 | first function is ≤5 lines, the file has ≥4 declarations and setup code |
| single-assignment |    10 | no functions; the wrapper sets exactly one binding                      |
| barrel            |     8 | declares nothing; the wrapper only loads other modules                  |
| big               |     8 | 20+ declarations                                                        |
| tiny              |     8 | one declaration besides the wrapper                                     |
| vendor-adjacent   |     8 | requires 2+ vendor files                                                |
| namespace         |     4 | read elsewhere as one module object                                     |
| other             |     4 | none of the above                                                       |

10 calls of 6 modules each. Every answer was judged by reading the module
(the same evidence the model saw, with the code opened where that was not
enough) against two things it would replace: today's FILE name (the first
function, from the base's fresh run) and today's WRAPPER name (the model's
per-identifier answer).

## Results — 60 modules read side by side

| new module name compared with    | better | equal | worse |
| -------------------------------- | -----: | ----: | ----: |
| today's file name                |     37 |    18 |     5 |
| today's wrapper name             |     38 |    17 |     5 |
| — barrels excluded (52): file    |     35 |    16 |     1 |
| — barrels excluded (52): wrapper |     36 |    15 |     1 |

By kind (vs file / vs wrapper, better-equal-worse):

| kind              | vs file | vs wrapper |
| ----------------- | ------- | ---------- |
| helper-first      | 9-0-1   | 7-3-0      |
| big               | 7-1-0   | 7-1-0      |
| vendor-adjacent   | 6-2-0   | 5-3-0      |
| namespace         | 4-0-0   | 3-1-0      |
| tiny              | 4-4-0   | 5-2-1      |
| other             | 3-1-0   | 3-1-0      |
| single-assignment | 2-8-0   | 6-4-0      |
| barrel            | 2-2-4   | 2-2-4      |

What the numbers say:

- **Files named after a helper are fixed.** 9 of 10 helper-first files get
  a name for what the file does (`round-to-nearest` → `color-utils`,
  `return-source` → `id-patterns`, `no-operation` → `worktree-ui`,
  `is-non-interactive-or-bg-session` → `structured-output-tool`). This is
  the failure the first-function rule cannot avoid, and the three losses of
  the earlier design were all this kind.
- **Big files get a concept, not their first function.**
  `is-valid-session-object` → `computer-use-lock`, `return-null-async` →
  `env-detection`, `validate-prompt-request` → `protocol-schema`.
- **Filler wrapper names disappear.** 13 of the 60 wrappers are filler
  today (`initializeAppComponents28`, `setupApplication63`,
  `setupEnvironment9`); none of the 52 non-barrel answers is.
- **Where the first function already was the file, nothing changes**
  (single-assignment: 8 of 10 equal — `tmux-setup-dialog`,
  `sessions-manager`).
- **Barrels are the weak spot.** A barrel declares nothing, so the prompt
  has only the names of what it loads. 4 of 8 answers were generic
  (`module-loader` twice, `app-initializer`, `environment-setup`); today's
  wrapper ask, which sees the barrel's call sites, did no worse. Barrels
  should stay out of the step (below).
- **One real miss outside barrels** (row 57): an MCP server list dialog
  named `agent-list-view` — the excerpt showed the helpers and the start of
  a 76-slot component, not enough to say "MCP". One answer said less than
  the model's wrapper name (row 2: `prompt-router-client` for the module
  that builds the prompt-routers _paginator_).

### All 60 rows

"today: file name" for a barrel is the wrapper's name (a barrel has no
other declaration, so the first-declaration rule lands on the wrapper).

|   # | kind              | today: file name                           | today: wrapper (model)                                          | new module name                     | vs file | vs wrapper | what the module is                                                                                         |
| --: | ----------------- | ------------------------------------------ | --------------------------------------------------------------- | ----------------------------------- | ------- | ---------- | ---------------------------------------------------------------------------------------------------------- |
|   1 | namespace         | `get-auto-retry-limit.js`                  | `initializeAppComponents28`                                     | `tool-search-config`                | better  | better     | tool-search module; first function is a retry-limit helper; wrapper is filler                              |
|   2 | tiny              | `prompt-router-client.js`                  | `initPromptRoutersPaginator`                                    | `prompt-router-client`              | equal   | worse      | the module builds the ListPromptRouters paginator; the model's wrapper name says so, the new name does not |
|   3 | tiny              | `endpoint-plugin5.js`                      | `deleteAutomatedReasoningPolicyBuildWorkflowCommandInitializer` | `automated-reasoning-policy-delete` | better  | equal      | file name is endpoint-plugin5; wrapper is exact but 60 chars                                               |
|   4 | helper-first      | `round-to-nearest.js`                      | `initializeTerminalUtilities`                                   | `color-utils`                       | better  | better     | color interpolation + spinner glyphs; first function roundToNearest is a helper                            |
|   5 | vendor-adjacent   | `aws-sdk-sig-v4-signer.js`                 | `setupAwsClient`                                                | `aws-client-options`                | better  | better     | SSO client runtime options (shuffled leg said sso-client-options, better still)                            |
|   6 | big               | `is-valid-session-object.js`               | `bootstrapRuntimeAndAnalyticsVal`                               | `computer-use-lock`                 | better  | better     | computer-use lock file                                                                                     |
|   7 | single-assignment | `crypto-operation.js`                      | `initCryptoValidation`                                          | `key-validation`                    | better  | equal      | JWT key-type validation                                                                                    |
|   8 | tiny              | `clone-array-buffer.js`                    | `initializeGlobalObjectsOnceVal`                                | `arraybuffer-clone`                 | equal   | better     | lodash cloneArrayBuffer; wrapper is filler                                                                 |
|   9 | namespace         | `retrieve-cached-value.js`                 | `initializeGlyphModule`                                         | `ansi-png-renderer`                 | better  | better     | ANSI text to PNG renderer                                                                                  |
|  10 | big               | `create-render-state.js`                   | `bootstrapApplicationData`                                      | `render-engine`                     | better  | better     | ink render-node-to-output engine                                                                           |
|  11 | vendor-adjacent   | `create-aws-sig-v4-auth.js`                | `initializeAwsSigV4Dependencies`                                | `auth-schemes`                      | equal   | better     | Bedrock auth-scheme provider; wrapper describes its imports                                                |
|  12 | helper-first      | `return-source.js`                         | `initializePatterns`                                            | `id-patterns`                       | better  | better     | agent/user id patterns; first two functions are identity helpers                                           |
|  13 | big               | `remove-bom.js`                            | `setupEnvironmentValue`                                         | `json-parse-cache`                  | better  | better     | JSON parsing helpers (jsonc, jsonl, cached read)                                                           |
|  14 | tiny              | `generate-handler-wrapper.js`              | `setupCoreHelpers`                                              | `collection-accumulator`            | better  | better     | lodash createAggregator                                                                                    |
|  15 | other             | `is-plain-object-strict.js`                | `lazyInitDescriptorSymbols`                                     | `descriptor-type-checks`            | better  | better     | protobuf descriptor type guards                                                                            |
|  16 | helper-first      | `process-lowercase-input.js`               | `initializeSurveyComponents`                                    | `option-prompt`                     | better  | equal      | yes/no/don't-ask-again option prompt                                                                       |
|  17 | helper-first      | `fetch-tool-permission-mode-var.js`        | `setupApplication63`                                            | `question-renderer`                 | better  | better     | AskUserQuestion question renderer                                                                          |
|  18 | other             | `wait-for-condition.js`                    | `initFsPromises`                                                | `shell-command`                     | better  | better     | ShellCommand (backgrounded process)                                                                        |
|  19 | single-assignment | `auto-mode-scan-task.js`                   | `initializeAutoModeScanTask`                                    | `auto-mode-scan-task`               | equal   | equal      | auto-mode scan task (stem already right)                                                                   |
|  20 | barrel            | `setup-utf8-encoder.js`                    | `setupUtf8Encoder`                                              | `module-loader`                     | worse   | worse      | barrel: empty init, loads a noop module; new name is generic (module-loader)                               |
|  21 | single-assignment | `use-option-selector.js`                   | `initializeCustomHook`                                          | `option-selector-hook`              | equal   | better     | option selector hook                                                                                       |
|  22 | namespace         | `get-default-azure-credential.js`          | `initializeCredentialExports`                                   | `azure-credential-exports`          | better  | better     | @azure/identity export surface                                                                             |
|  23 | barrel            | `initialize-message-processing.js`         | `initializeMessageProcessing`                                   | `event-stream-processor-loader`     | better  | better     | barrel: loads the event-stream codec pieces                                                                |
|  24 | single-assignment | `ordered-map.js`                           | `OrderedMapFactory`                                             | `ordered-map`                       | equal   | better     | OrderedMap class (wrapper said Factory)                                                                    |
|  25 | single-assignment | `input-stream-serializer.js`               | `inputStreamSerializerFactory`                                  | `input-stream-serializer`           | equal   | better     | InputStreamSerializer class                                                                                |
|  26 | big               | `sts-sdk.js`                               | `initializeStsTypes`                                            | `aws-sts-sdk`                       | equal   | equal      | STS SDK schema types                                                                                       |
|  27 | single-assignment | `get-auto-background-timeout.js`           | `configureRuntimeEnvironment`                                   | `mcp-auto-background`               | better  | better     | MCP tool auto-background                                                                                   |
|  28 | big               | `initialize-hljs.js`                       | `setupRuntimeVar`                                               | `ansi-color-formatter`              | better  | better     | syntax-highlight theme to ANSI colors                                                                      |
|  29 | namespace         | `generate-text-response.js`                | `initColorCommand`                                              | `color-command`                     | better  | equal      | /color command                                                                                             |
|  30 | vendor-adjacent   | `otlp-exporter-base-provider.js`           | `initOtlpLogExporter`                                           | `otlp-log-exporter`                 | better  | equal      | OTLP log exporter                                                                                          |
|  31 | helper-first      | `get-voice-mode.js`                        | `setupVoiceComponents`                                          | `voice-ui`                          | better  | equal      | voice indicator components                                                                                 |
|  32 | helper-first      | `get-workflow-file-path.js`                | `initializeRuntimeAndPaths`                                     | `workflow-storage`                  | better  | better     | workflow snapshot storage                                                                                  |
|  33 | big               | `return-null-async.js`                     | `initializeAppEnvironmentData`                                  | `env-detection`                     | better  | better     | environment detection                                                                                      |
|  34 | big               | `register-event-handler.js`                | `initializeEngineAndGlobals`                                    | `react-debug-reconciler`            | better  | better     | ink React reconciler host config (new adds a misleading "debug")                                           |
|  35 | vendor-adjacent   | `otlp-exporter-base-module.js`             | `initOtlpTraceExporter`                                         | `otlp-trace-exporter`               | better  | equal      | OTLP trace exporter                                                                                        |
|  36 | single-assignment | `terminal-focus-event.js`                  | `getTerminalFocusEventClass`                                    | `terminal-focus-event`              | equal   | better     | TerminalFocusEvent class (wrapper used a get verb)                                                         |
|  37 | barrel            | `initialize-http-error-handling.js`        | `initializeHttpErrorHandling`                                   | `module-loader`                     | worse   | worse      | barrel: new name generic (module-loader)                                                                   |
|  38 | big               | `validate-prompt-request.js`               | `initializeSchemas`                                             | `protocol-schema`                   | better  | better     | MCP protocol schemas                                                                                       |
|  39 | tiny              | `build-urlsearch-params.js`                | `setupEnvironmentResult`                                        | `url-search-params`                 | equal   | better     | axios toURLEncodedForm                                                                                     |
|  40 | barrel            | `init-analytics-and-user-id-patterns.js`   | `initAnalyticsAndUserIdPatterns`                                | `app-initializer`                   | worse   | worse      | barrel: new name generic (app-initializer)                                                                 |
|  41 | vendor-adjacent   | `sanitize-object.js`                       | `initializeWorkflowSandbox`                                     | `workflow-sandbox`                  | better  | equal      | workflow script sandbox                                                                                    |
|  42 | vendor-adjacent   | `aws-sdk-sig-v4-signer-lib.js`             | `initializeSdkEnvironment`                                      | `aws-client-config`                 | better  | better     | Bedrock client runtime config                                                                              |
|  43 | vendor-adjacent   | `validate-manifest.js`                     | `initializeModulesAndSchemasVal`                                | `manifest-validator`                | equal   | better     | .mcpb manifest validate/clean                                                                              |
|  44 | tiny              | `check-entry-exists.js`                    | `ktlInit`                                                       | `map-contains`                      | equal   | better     | lodash mapCacheHas; wrapper half-minified (ktlInit)                                                        |
|  45 | vendor-adjacent   | `detect-non-determinism.js`                | `initializeRuntimeEnvironment3`                                 | `workflow-task-manager`             | better  | better     | local workflow task handlers                                                                               |
|  46 | helper-first      | `no-operation.js`                          | `setupEnvironment9`                                             | `worktree-ui`                       | better  | better     | exit/goodbye flow with worktree prompt                                                                     |
|  47 | tiny              | `cognito-identity-endpoint.js`             | `initUnlinkIdentityCommand`                                     | `cognito-unlink`                    | better  | equal      | Cognito UnlinkIdentity command                                                                             |
|  48 | tiny              | `placeholder-fn.js`                        | `sectionTypeEnum`                                               | `section-enum`                      | better  | better     | Section enum (HEADER/TRAILER); file was placeholder-fn                                                     |
|  49 | other             | `extract-axios-error-message.js`           | `bootstrapCoreModulesRef`                                       | `usage-credits-admin`               | better  | better     | usage-credits admin request                                                                                |
|  50 | barrel            | `lazy-credential-initializer.js`           | `lazyCredentialInitializer`                                     | `http-client-base`                  | equal   | equal      | barrel: loads HttpClientBase only                                                                          |
|  51 | helper-first      | `fetch-expanded-view.js`                   | `setupTerminalEnvironmentRef`                                   | `view-state-manager`                | better  | better     | app-level key actions (todos/transcript toggles)                                                           |
|  52 | single-assignment | `abort-operation-error.js`                 | `abortErrorClass`                                               | `abort-error`                       | equal   | better     | AbortError class                                                                                           |
|  53 | helper-first      | `is-non-interactive-or-bg-session.js`      | `initializeApplicationValue`                                    | `structured-output-tool`            | better  | better     | StructuredOutput tool                                                                                      |
|  54 | single-assignment | `sessions-manager.js`                      | `initializeSessionsManager`                                     | `sessions-manager`                  | equal   | equal      | sessions API client                                                                                        |
|  55 | barrel            | `initialize-analytics-and-dependencies.js` | `initializeAnalyticsAndDependencies`                            | `environment-setup`                 | worse   | worse      | barrel: new name generic (environment-setup)                                                               |
|  56 | barrel            | `initialize-default-client-config-lazy.js` | `initializeDefaultClientConfigLazy`                             | `retry-strategy`                    | equal   | equal      | barrel: retry-strategy pieces                                                                              |
|  57 | helper-first      | `extract-mcp.js`                           | `setupApplication49`                                            | `agent-list-view`                   | worse   | equal      | MCP server list dialog; new name says agent-list-view (wrong); file extract-mcp at least says mcp          |
|  58 | single-assignment | `tmux-setup-dialog.js`                     | `initTmuxSetupDialog`                                           | `tmux-setup-dialog`                 | equal   | equal      | tmux setup dialog                                                                                          |
|  59 | barrel            | `initialize-app-modules-val.js`            | `initializeAppModulesVal`                                       | `api-services`                      | better  | better     | barrel: Anthropic API client services                                                                      |
|  60 | other             | `update-monitoring-notice.js`              | `initAutoModeConfig`                                            | `auto-mode-notice`                  | equal   | equal      | auto-mode notice store                                                                                     |

The wrapper the new name would give is `init` + the name in PascalCase
(`initToolSearchConfig`, `initComputerUseLock`). The longest in the sample
is `initAutomatedReasoningPolicyDelete`; answers average 2.5 words, never
more than 4 (today's longest stem-derived wrapper was 55 characters).

## Does the model give the same name twice?

Four legs over the same 60 modules:

| leg                                                | same answer as the first leg |
| -------------------------------------------------- | ---------------------------: |
| exact repeat (same batches, same order)            |                        52/60 |
| the 2.1.215 base (55 of 60 prompts byte-identical) |                        47/60 |
| same 2.1.216 modules, batched in a different order |                        26/60 |

Temperature 0 on this server is not fully repeatable (8 of 60 moved on an
identical re-ask), and the answer depends a lot on which other files share
the call. Nearly all the changes are synonyms or refinements
(`env-detection`/`env-detect`, `react-debug-reconciler`/`react-debug`,
`aws-client-options`/`sso-client-options`); the ones that changed meaning
were barrels. It matters less than it looks, because the step only ever
asks a module ONCE — a matched module carries its path and wrapper name
(the wrapper carry is 99–100% per hop today, `lazy-init-naming.md`), so the
draw never repeats across versions. It does mean: keep the batching a pure
function of the module order (the existing `split_namer_batches` already
is), so a replayed run asks the same questions.

## Sizing

### Calls

|                                                                                                       |                                   today |                 proposed |
| ----------------------------------------------------------------------------------------------------- | --------------------------------------: | -----------------------: |
| fresh run (2.1.215 base, 77,434 calls) — calls that asked only about wrappers (`lazy-init-naming.md`) |                                   5,952 |                        0 |
| fresh run — module-name calls (4,657 non-barrel modules, 8 per call)                                  |                                       0 |                     ~583 |
| **fresh run, net**                                                                                    |                                         | **≈ −5,370 calls (−7%)** |
| warm hop — mint-namer calls                                                                           |                                       1 |                        0 |
| warm hop — new modules to name                                                                        | 27–94 per hop (the mint namer's counts) |               4–12 calls |
| warm hop — wrapper asks for new modules                                                               |            ~100 (`lazy-init-naming.md`) |                        0 |

Per module the prompt costs ~470 tokens in and ~26 out (measured: 28,180 /
1,568 tokens for 60 modules). Entry size over the whole 2.1.216 population:
mean 1,721 characters, 90th percentile 2,765. Eight per call stays near
22K characters, well inside the 32K-token context. One entry reached 23,719
characters (a schema module declaring ~200 names on one `var` line): the
built version must cap the declaration list by names, not by statements.

### Answers the checks would refuse

`accept_proposed_name` (identifier-shaped, ≤40 chars, not a generic word,
no counter number, no leading stopword) refused **0 of 240** answers across
the four legs. It does not catch `module-loader`, `app-initializer` or
`environment-setup` (only whole-word generics are on its list) — another
reason to keep barrels out rather than widen the list.

The answer-quality checks for the wrapper name are the ones the naming
stage already applies to any rename (minified-looking names, a name already
taken in the scope) through the validated renamer — the step's wrapper
names go through the same single path, never a direct rename.

### Duplicates

Within the 60 answers: one duplicate (`module-loader` ×2, both barrels).
Against the 4,851 existing file names: 1–2 answers per leg (`http-client-base`,
`event-stream-processor`). Extrapolated, roughly 2–3% of a fresh run's
module names (~100–150 of 4,657) would collide with another module's name.
The two halves collide differently:

- **file**: only a clash in the SAME folder matters; `claim_path` already
  resolves it (and with folders, most cross-folder repeats are fine);
- **wrapper**: all wrappers live in one scope, so a repeat must not be
  applied. Resolution, in order: (1) inside one call the prompt demands
  distinct names; (2) across calls, the first module in bundle order keeps
  the name; (3) the rest are asked ONCE more in a small batch that lists the
  taken names (the disclosed-retry shape the naming stage already uses);
  (4) anything still colliding falls back to `init` + the mechanical stem,
  and its file to the mechanical stem — today's behaviour.

### Barrels

169 of 4,826 modules (3.5%) declare nothing. Keep them out: their wrapper is
asked in the waves exactly as today (the ask sees the call sites), and their
file and folder keep following the wrapper's name, as they already do.

## Where it runs

Today, in a fresh run:

1. naming waves ask every identifier, wrappers included;
2. naming floor + sweep (fresh run: before `generate`) ask what is still
   minified;
3. the split extracts the modules (`twins::fossil`), names every file
   `module_stem` (first function), folders from the anchoring module's stem;
4. on warm hops, the split's mint namer polishes unmatched modules' file
   names.

Proposed:

1. **Before the first wave** — the naming stage extracts the module
   boundaries with the split's own owner (`twins::fossil`), and marks the
   wrapper of every NEW, non-barrel module as "named by the module step":
   no wave asks it. "New" = the prior-version transfer did not carry the
   wrapper's name (on a fresh run: every module).
2. **The waves run as today.** Every other name in the module is final by
   their end, which is what the step's evidence needs.
3. **The module step**, after the waves and the library-prefix pass, BEFORE
   the naming floor and sweep: one batched pass over the marked modules;
   each accepted, non-colliding answer renames the wrapper to `init<Name>`
   through the validated renamer, recorded on the trail as a pipeline
   decision (its own tier, beside `toolchain-plumbing`). A wrapper whose
   answer is refused or collides stays minified and the floor/sweep asks it
   — the existing disclosed path, so nothing is left unnamed.
4. **The split** reads the module name back from the wrapper: for a module
   it cannot inherit, the file stem is the wrapper's name with `init`
   dropped, kebab-cased (`initComputerUseLock` → `computer-use-lock.js`),
   checked by `accept_proposed_name`, else `module_stem` as today. The same
   stems feed folder naming (`infer_fossil_placements`), so a folder
   anchored on a new module is named after what it does, not its first
   function. The wrapper's name is the single record of the module's name:
   no side table, and the file can never disagree with its wrapper.
5. **The mint namer goes away for fossil bundles** — the module step does
   its job, earlier and with better evidence. The clustered split (bundles
   without module records) keeps the existing file/folder namer unchanged.

What does NOT change: a module the split matches to the prior keeps its
prior path, verbatim (`assign_fossil` only names `file_of_module[i] ==
None`); a wrapper the transfer carries keeps its name. The two matchers
(the naming transfer for the wrapper, the split's fossil matcher for the
file) can disagree on a few modules per hop; both cases stay safe — a
carried wrapper on an unmatched file gives the file the carried module name
(step 4), and a matched file with a newly named wrapper keeps its path.

One thing that DOES need changing with it: the split's
`stem-corroborated` match tier compares a module's fresh `module_stem` with
the prior FILE stem. Once files carry module names those never agree, and
the tier (0–34 statements per hop today; already blind for mint-named
files) goes dead. It should compare against the prior's recorded mechanical
stem (one more field in the fossil ledger) — to be done in the same change
and checked with the placement trail counts.

Cost of the order: while the waves run, the few prompts that show a
wrapper call (dynamic-import sites, usage lines of the bindings it sets)
show it under its minified name — the same cost `lazy-init-naming.md`
accepted, already guarded by the borrowed-stem refusal.

## Recommendation

Build it, barrels excluded, as a module kind of the existing file namer.

Expected effects (to check in the scratch eval):

| measure                              | expected                                                         | how to check                                                    |
| ------------------------------------ | ---------------------------------------------------------------- | --------------------------------------------------------------- |
| filler wrapper names per scored tree | ~950 → barrels and fallbacks only (~200)                         | `derive.py`'s filler count on the trees                         |
| half-minified wrapper names          | 8–39 → ~0                                                        | same                                                            |
| files named after a helper           | most of them renamed for what the file does (9/10 in the sample) | re-read a fresh sample                                          |
| LLM calls, fresh run                 | ≈ −5,370 (−7%)                                                   | run report                                                      |
| LLM calls, scored hops               | ≈ −100                                                           | run report                                                      |
| novel / realLn                       | 0 — naming only                                                  | leaderboard (band is zero)                                      |
| noiseLn / treeLn on scored hops      | inside the bands: matched modules keep path and wrapper          | leaderboard vs `noise-bands.json`                               |
| existing paths renamed on a warm hop | 0                                                                | placement trail: every inherited file's path equal to its prior |
| cold self-hop                        | inside the reference range                                       | self-hop gate                                                   |
| `stem-corroborated` tier             | same count as before (after the ledger fix)                      | placement trail tier counts                                     |

Two cautions on reading the eval. By rule 11 the noise KPIs cannot
resolve a naming change like this one, so the counts measured on the trees
(filler, half-minified, the re-read sample) are what judge it. And the
scratch BASES are fresh runs: their file names all change at once, so the
first eval after the change compares two trees that were both named by the
new step — that is the point of scratch bases — but any comparison against
a label scored before it mixes two naming schemes for every file.

Risks:

- **thin evidence** — a module whose meaning lives in a large component the
  excerpt only starts (row 57) can be named wrong; a wrong name is worse
  than a filler one. A bigger excerpt for big modules, or showing who
  imports the module, are the levers; neither is sized yet.
- **draw variance** — the first name a module gets is a draw (52/60
  repeatable); it is then carried forever. Same as every other model name.
- **collisions** — ~2–3% need the retry or the fallback.
- **prototype limits** — the evidence was rebuilt from the split files with
  regexes (a few multi-line function signatures were missed, some imports
  showed as `importedModule`); the built version reads the bundle's syntax
  tree and should only show more.

## Open questions for Andrew

- Barrels: leave them to the waves (recommended), or ask them too with a
  stronger "no generic words" check?
- Folder names would start following module names on new folders (step 4).
  Wanted, or keep folders on the mechanical stem for now?
- The existing namer's system prompt says "CLI tool" — make it "program"
  for every kind (changes the cluster split's prompts and so its cache
  keys), or only for the new module kind?
