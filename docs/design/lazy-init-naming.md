# Naming lazy-init wrappers after what they set up — design and sizing

STATUS: design + sizing only (2026-10-05). Nothing below is built except
the measurement scripts. Part (a) of the same decision — the lazy-init
HELPER gets a pipeline-chosen name, `__esm` — is built (finding #86,
`naming::plumbing`); this document is part (b).

Andrew, 2026-10-05: "lazy-init wrappers should be named after what they set
up … we definitely want these named somehow" — design and size it from data
first, report before building.

Evidence: `/work/lazy-init-naming-2026-10-05/` (scripts `population.py`,
`sources.py`, `derive.py`, `stability.py`, `table.py`, `commands.py`,
`sample_derived.py`; the 40-sample is `sample40-derived.txt`). Population:
the eight trees of `candidate-5d4b2d9a-scratch` — the four scored versions
and their four scratch bases.

## What a lazy-init wrapper is

Bun (and esbuild) turn every source file that uses ES modules into one
function that runs the file's top-level code the first time someone needs
it:

```js
var __esm = (fn, res) => () => (fn && (res = fn((fn = 0))), res); // the helper
var initGetSystemPrompt = __esm(() => {
  // one wrapper per source file
  initErrorClasses(); // the files it imports, first
  setupMessage = buildSetupMessage(); // its own top-level assignments
});
```

Callers run `initGetSystemPrompt()` before using anything from that file, or
`(initFoo(), fooNamespace)` where the original code had a dynamic
`import("./foo")`. Unminified esbuild output names them `init_<file name>`
— the wrapper's honest name is "the initializer of file X".

## The population

| tree           | wrappers | model name is filler | half-minified (`initAos`) | fully minified |
| -------------- | -------: | -------------------: | ------------------------: | -------------: |
| 2.1.85 (base)  |    3,237 |                  508 |                         9 |              0 |
| 2.1.86         |    3,249 |                  533 |                         8 |              0 |
| 2.1.118 (base) |    3,542 |                  741 |                        16 |              0 |
| 2.1.119        |    3,613 |                  774 |                        16 |              0 |
| 2.1.197 (base) |    4,432 |                1,051 |                        32 |              0 |
| 2.1.198        |    4,447 |                1,060 |                        32 |              0 |
| 2.1.215 (base) |    4,796 |                  938 |                        39 |              2 |
| 2.1.216        |    4,826 |                  956 |                        39 |              2 |

"Filler" is a name built only from generic words (`initializeApplication29`,
`setupApplicationItem`, `bootstrapApp7`, `initCoreLazy`) — the regex is in
`derive.py`. Roughly one wrapper in five gets one. A further share of the
non-filler names have the wrong verb (`getTerminalContext`,
`createEncodingStream`, `mergeDeveloperIdentities` — calling a wrapper
returns nothing) or describe the imports rather than the file
(`loadReactAndColorLib`, `initializeAnalyticsAndCrypto`).

What a wrapper sets up (2.1.216; one wrapper can be in several rows):

| what the wrapper does                                     | wrappers | share |
| --------------------------------------------------------- | -------: | ----: |
| lands in exactly one split file                           |    4,812 | 99.7% |
| belongs to a recorded module with other declarations      |    4,657 | 96.5% |
| assigns two or more of its file's bindings                |    2,241 | 46.4% |
| assigns exactly one binding                               |    1,265 | 26.2% |
| only calls other wrappers (its file is functions only)    |    1,232 | 25.5% |
| is read as a module object, `(initFoo(), fooNamespace)`   |      408 |  8.5% |
| builds a slash command (`type: "local", name: "upgrade"`) |       94 |  1.9% |
| a barrel: only calls other wrappers AND declares nothing  |      169 |  3.5% |

Every split file holds exactly one wrapper: the wrapper IS its file's
initializer, which is why the file is the natural name source.

## Candidate naming sources

Each source makes `init` + the source word in PascalCase (a leading
`init`/`initialize` on the source is dropped first). Counts on 2.1.216
(4,826 wrappers); 2.1.198, 2.1.119 and 2.1.86 read within a point of these.

| source                                                                                                              | covers | would change the model's name | same name twice | name already taken | identical across 215→216 (when derivable both sides) |
| ------------------------------------------------------------------------------------------------------------------- | -----: | ----------------------------: | --------------: | -----------------: | ---------------------------------------------------: |
| **module stem** — the file's first function/class, else its first declared variable (the split's own `module_stem`) |  96.5% |                         4,580 |              32 |                  6 |                                                98.3% |
| split file name (after naming — see below)                                                                          |  99.7% |                         4,737 |              30 |                  2 |                                                    — |
| module object (namespace)                                                                                           |   8.5% |                           382 |              14 |                  0 |                                               100.0% |
| the one binding it assigns                                                                                          |  26.2% |                         1,136 |              20 |                  0 |                                               100.0% |
| slash command name                                                                                                  |   1.9% |                     (not run) |               — |                  — |                                                    — |
| callee chain (barrels)                                                                                              |   3.5% |                    not sized¹ |                 |                    |                                                      |

¹ A barrel only re-runs other files' initializers; nothing in it names it
except the folder the split anchors on it, which is decided after naming.

The split FILE name starts from the module stem in a fresh run
(`place::assign::fossil::module_stem` kebab-cased, then polished by the mint
namer) and is an inherited path otherwise — but files are named AFTER
naming, so the file itself cannot be the source. The module stem is the same
rule computed inside the naming stage, from the recorded module boundaries
(`twins::fossil`, the statements between one wrapper and the previous one)
and the names current at that moment.

### Quality — 40 wrappers read side by side (2.1.216, seed 11)

`sample40-derived.txt`. Judged on: does the name say which file it
initializes, and is the verb honest — read from each wrapper's file, the
bindings it assigns and the declarations of its module (the sample lines),
with the code opened where those disagreed.

| verdict                               | count |
| ------------------------------------- | ----: |
| module-stem name better               |    26 |
| equally good                          |    10 |
| model's name better                   |     3 |
| no stem (a barrel) — model name stays |     1 |

Typical wins: `initializeApplication29` → `initOAuthFlow`,
`setupApplicationItem` → `initValidateOAuthProfile`, `getTerminalContext` →
`initContextCreator`, `ProtoNestInFileClass` → `initNestInFileClass`. The three
losses are files whose first function is a small helper while the wrapper's
real job is one assignment: `initializeEndOfWordRegex` → `initCloneRegex`,
`initializeToolSchemas` → `initIsEscaped`, `initializeAllowedModes` →
`initIsExcludedQuerySource` (the one-assigned source gives
`initAllowedModes` there). Weak spots of the stem: very long stems
(60 of 4,657 are 40+ characters, e.g.
`initCancelAutomatedReasoningPolicyBuildWorkflowEndpoint`), a handful of
stems that are themselves minified (`initOrc`, `initEDd` — 5 in 2.1.216;
refused below), and CONSTANT_CASE stems that need re-casing
(`initMODELTOKENLIMITS` → `initModelTokenLimits`).

## Replace the model, or only fix bad answers?

Ladder for both: the module object when there is one, else the module stem;
duplicates, taken names and stems that are short or minified fall back to
the model.

| tree     | derivable | duplicates | taken | REPLACE: names changed | ONLY-WHEN-BAD: names changed (of bad) |
| -------- | --------: | ---------: | ----: | ---------------------: | ------------------------------------: |
| 2.1.85b  |     3,045 |         30 |     6 |                  2,978 |                             497 (517) |
| 2.1.86   |     3,061 |         30 |     6 |                  2,994 |                             518 (541) |
| 2.1.118b |     3,344 |         28 |     7 |                  3,269 |                             721 (757) |
| 2.1.119  |     3,446 |         30 |     6 |                  3,373 |                             766 (790) |
| 2.1.197b |     4,232 |         38 |     3 |                  4,153 |                         1,047 (1,083) |
| 2.1.198  |     4,246 |         38 |     2 |                  4,168 |                         1,054 (1,092) |
| 2.1.215b |     4,568 |         52 |     6 |                  4,471 |                             942 (979) |
| 2.1.216  |     4,596 |         52 |     6 |                  4,498 |                             960 (997) |

- **REPLACE** never asks the model about a derivable wrapper. It saves the
  asks: in the 2.1.215 scratch base 8,013 of 37,675 module-level asked
  identifiers (21%) were wrappers, and 5,952 of the run's 76,334 calls (7.8%)
  asked about nothing else. Every wrapper reads the same way (the bundler's
  own `init<File>` convention).
- **ONLY-WHEN-BAD** asks first, then overrides answers a vocabulary rule
  calls filler. It keeps every call, needs a filler judge (a word list —
  exactly the kind of rule that drifts), and overriding an answer after the
  fact is closer to "rewriting the model's answer" than declining to ask.
  It fixes ~1,000 names per tree and leaves the wrong-verb and
  imports-not-file names (most of the 26 wins above were not filler).

## Stability across versions (215 → 216, and the other three hops)

Wrappers paired by the file they land in (a matched module inherits its
prior path; one wrapper per file).

| hop       | paired | model name identical | model churned (lines²) | if the stem were re-derived EVERY version: churned (lines²) |
| --------- | -----: | -------------------: | ---------------------: | ----------------------------------------------------------: |
| 85 → 86   |  3,091 |                99.0% |                23 (65) |                                                 215 (1,408) |
| 118 → 119 |  3,495 |                99.9% |                  2 (4) |                                                    28 (712) |
| 197 → 198 |  4,384 |                99.9% |                  4 (9) |                                                    69 (625) |
| 215 → 216 |  4,784 |               100.0% |                  2 (5) |                                                    78 (447) |

² declaration + call sites of the renamed wrappers.

Once a wrapper is named, the prior-version transfer already carries its name
almost perfectly. A stem-derived name re-computed every version would be
LESS stable (98.3% on 215→216): it moves whenever the file's first function
is renamed, reordered or a new one is added in front — 447–1,408 changed lines
per hop, against 4–65 today. The answer is to derive ONCE, when the wrapper
is first named, and carry it afterwards like any name. Then cross-version
stability is today's (the transfer's), and the deterministic name only
removes the draw in the version that first names it.

## Recommendation

1. **REPLACE, derive-once.** A lazy-init wrapper the run names itself (not
   carried by the transfer) gets `init<Source>` from the ladder:
   module object → module stem → the model. Precision rules: only a wrapper
   whose current name is minifier-made (an unminified esbuild build already
   says `init_foo`); the source re-cased (CONSTANT_CASE, a leading
   `init`/`initialize` dropped); a source that is itself below the floor or
   ≤3 letters after the prefix, a duplicate, or a name already taken falls
   back to the model. Applied as a deterministic decision (trail tier, like
   `toolchain-plumbing`), never by rewriting an answer.
2. **When it runs.** The source names must be final, so: mark derivable
   wrappers before the first wave (so no wave asks them), apply after the
   last wave and before the floor/sweep. A fallback wrapper is then still
   minifier-named and the coverage sweep asks it — the existing
   disclosed path. Cost: during the waves, the few prompts that show a
   wrapper call (the 680 dynamic-import sites, assigned bindings' usage
   lines) show it under its minified name; the borrowed-stem refusal already
   guards answers that copy one.
3. **One shape owner.** Module boundaries come from `twins::fossil` (the
   split's), the stem rule from `place::assign::fossil::module_stem` — the
   naming stage calls them, it does not grow a second copy.

Expected effect (scratch eval):

| measure                              | expected                                                             |
| ------------------------------------ | -------------------------------------------------------------------- |
| filler wrapper names per scored tree | ~950 → the fallback share only (~230 asked, of which ~1 in 5 filler) |
| half-minified wrapper names          | 8–39 → 0 (a stem below the floor is refused)                         |
| LLM calls, fresh run                 | −~6,000 (−7.8%); scored hops −~100                                   |
| novel / realLn                       | 0 (naming only)                                                      |
| noiseLn / treeLn                     | ~0: wrapper names are already carried at 99–100%; inside the bands   |
| cold self-hop                        | flat or slightly down: one fewer independent draw per file           |

The gain is READABILITY of every file's initializer, not a noise KPI:
by rule 11 the eval cannot resolve this lever's effect on noise, and the
filler / half-minified counts above (measured on the trees, not the KPIs)
are the measure to judge it by.

Risks: a file whose first function is a minor helper names its wrapper after
the helper (3 of 40); long stems make long names; barrels (3.5%) stay with the
model.

Open questions for Andrew:

- the one-binding source beats the stem in the three losses above; a
  rung "exactly one assigned binding AND no function in the file" would
  catch them (sizing: 1,265 single-assignment wrappers; the overlap with
  function-free files is not yet measured);
- slash-command wrappers (94) could read `init<Name>Command` from the
  command's own `name:` literal — the model already names these well.
