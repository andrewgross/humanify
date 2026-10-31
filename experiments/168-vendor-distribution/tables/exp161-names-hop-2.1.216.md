# exp161-names-hop — 2.1.215->2.1.216 (names mode)

## Run status

| check                     | value                                                                                                                    |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| runStatus                 | MISSING (/work/exp161-hop/experiments/034-eval-harness/results/exp161-names-hop/2.1.216-run-status.json — card dir gone) |
| selfHopCard               | missing                                                                                                                  |
| tree 2.1.216              | present                                                                                                                  |
| tree 2.1.216-selfhop      | present                                                                                                                  |
| tree 2.1.216-selfhop-warm | present                                                                                                                  |

## Consistency checks (each counter shown able to fire)

| check                                  | value                                                                                                                                                                                                                    |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| factoryShapePairing                    | 1578/1618 fresh vendor factory files paired to a raw factory by shape (1555/1595 prior); record order would have agreed for 1417                                                                                         |
| carryOverReconstructed                 | 1595 factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)                                                                               |
| eraVendorStatements                    | 103                                                                                                                                                                                                                      |
| factoriesWithContestedClaim            | 6                                                                                                                                                                                                                        |
| indexLabelledFreshFactories            | 1383                                                                                                                                                                                                                     |
| restorationApplicationSlots            | 65828                                                                                                                                                                                                                    |
| restorationApplicationSlotsInFactories | 37252                                                                                                                                                                                                                    |
| eraTrailFileMapping                    | 2019/2049 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 0 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                        |
| eraRestorationFrozen                   | null                                                                                                                                                                                                                     |
| mintedCensusTotal                      | 685                                                                                                                                                                                                                      |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 903,201 | 48,335,074 | 100.0%         |
| src/    | 4,845 | 874,152 | 41,902,960 | 96.8%          |
| vendor/ | 1,654 | 24,054  | 6,160,211  | 2.7%           |
| other   | 4     | 4,995   | 271,903    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,654 | 24,054 | 6,160,211 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,595 | 6,712  | 5,575,298 | 90.5%                 |
| index claim moved by package                            | 18    | 486    | 21,545    | 0.3%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 22    | 92     | 19,822    | 0.3%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 18    | 3,105  | 102,284   | 1.7%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 13,659 | 441,262   | 7.2%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                                                                     | files | lines  | bytes     |
| --------------------------------------------------------------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: claimed                            | 797   | 4,017  | 3,692,599 |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: unclaimed (exp157 rule)            | 546   | 1,500  | 1,066,227 |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm                                                          | 207   | 915    | 516,982   |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code                                                     | 1     | 13,659 | 441,262   |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: claim unknown (factory not paired) | 39    | 257    | 278,060   |
| era (app) statements filed into vendor by placement :: era file: fossil:unique-signature+prior-path-by-hash, fossil:unique-signature    | 9     | 2,837  | 90,666    |
| whole-module factory, named this run (model/declared) :: llm, decided this run                                                          | 22    | 92     | 19,822    |
| index claim moved by package :: era file: library-index (region claims not recorded)                                                    | 17    | 481    | 19,579    |
| inherited from prior by hash (factory carry-over) :: carried; origin label declared                                                     | 2     | 13     | 19,459    |
| era (app) statements filed into vendor by placement :: era file: fossil:unique-signature, fossil:unique-signature+prior-path-by-hash    | 3     | 192    | 8,319     |
| era (app) statements filed into vendor by placement :: era file: fossil:edge-corroborated                                               | 3     | 45     | 2,030     |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback                                                     | 4     | 10     | 1,971     |
| index claim moved by package :: index, decided this run, claimed                                                                        | 1     | 5      | 1,966     |
| era (app) statements filed into vendor by placement :: era file: fossil:high-overlap-unique                                             | 3     | 31     | 1,269     |

### per package (top 25 by bytes, then the long tail)

| package                             | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                       |
| ----------------------------------- | ----- | ------ | --------- | -------------- | --------------------------------------------------- |
| highlight.js                        | 193   | 397    | 1,137,114 | 18.5%          | inherited from prior by hash (factory carry-over)   |
| (vendor root)                       | 5     | 13,669 | 443,233   | 7.2%           | the vendor record (\_bun-modules.json)              |
| @grpc/grpc-js                       | 65    | 407    | 373,839   | 6.1%           | inherited from prior by hash (factory carry-over)   |
| node-forge                          | 41    | 292    | 355,997   | 5.8%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/otlp-transformer     | 20    | 73     | 246,841   | 4.0%           | inherited from prior by hash (factory carry-over)   |
| @mixmark-io/domino                  | 16    | 99     | 220,891   | 3.6%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/sdk-metrics          | 99    | 445    | 175,967   | 2.9%           | inherited from prior by hash (factory carry-over)   |
| acorn                               | 1     | 10     | 152,019   | 2.5%           | inherited from prior by hash (factory carry-over)   |
| google-auth-library                 | 43    | 217    | 132,590   | 2.2%           | inherited from prior by hash (factory carry-over)   |
| react-error                         | 1     | 15     | 125,506   | 2.0%           | inherited from prior by hash (factory carry-over)   |
| @xmldom/xmldom                      | 8     | 38     | 114,802   | 1.9%           | inherited from prior by hash (factory carry-over)   |
| semver                              | 113   | 500    | 113,069   | 1.8%           | inherited from prior by hash (factory carry-over)   |
| protobufjs                          | 27    | 160    | 111,823   | 1.8%           | inherited from prior by hash (factory carry-over)   |
| @aws-sdk/core                       | 4     | 46     | 104,882   | 1.7%           | inherited from prior by hash (factory carry-over)   |
| sharp                               | 15    | 103    | 91,696    | 1.5%           | inherited from prior by hash (factory carry-over)   |
| xmlbuilder                          | 33    | 177    | 89,832    | 1.5%           | inherited from prior by hash (factory carry-over)   |
| @smithy/core                        | 6     | 40     | 88,603    | 1.4%           | inherited from prior by hash (factory carry-over)   |
| openid-client                       | 28    | 143    | 79,111    | 1.3%           | inherited from prior by hash (factory carry-over)   |
| vscode-jsonrpc                      | 14    | 93     | 76,567    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/core                 | 62    | 227    | 75,269    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| fs-extra                            | 54    | 284    | 74,226    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/semantic-conventions | 6     | 14     | 72,345    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| jose                                | 4     | 2,072  | 64,266    | 1.0%           | era (app) statements filed into vendor by placement |
| ajv                                 | 34    | 173    | 53,818    | 0.9%           | inherited from prior by hash (factory carry-over)   |
| fast-xml-parser                     | 1     | 8      | 50,461    | 0.8%           | inherited from prior by hash (factory carry-over)   |
| long tail (283 packages)            | 761   | 4,352  | 1,535,444 | 24.9%          | —                                                   |

### vendor-root loose files

| mechanism                                         | files | lines  | bytes   |
| ------------------------------------------------- | ----- | ------ | ------- |
| the vendor record (\_bun-modules.json)            | 1     | 13,659 | 441,262 |
| inherited from prior by hash (factory carry-over) | 4     | 10     | 1,971   |

## 2. How identifiers are named

### vendor/ factory bodies (each binding zipped against the raw bundle's factory)

| name source                                                                          | bindings | share |
| ------------------------------------------------------------------------------------ | -------- | ----- |
| TOTAL                                                                                | 53,160   | 100%  |
| restored from the library index (file answered this run; bytes inherited from prior) | 27,218   | 51.2% |
| left as bundled (minifier's name)                                                    | 11,432   | 21.5% |
| carried from prior: file inherited, still the prior's minifier name                  | 10,608   | 20.0% |
| factory not paired to the raw bundle                                                 | 3,792    | 7.1%  |
| restored from the library index (vendored body)                                      | 110      | 0.2%  |

### vendor/ era (app) statements (strategy trail, by terminal step)

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 72       | 100%  |
| left as bundled (no final name, or kept the minifier's) | 66       | 91.7% |
| post-split reconcile (finish rule)                      | 6        | 8.3%  |

### src/ (strategy trail, by terminal step) — the contrast

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 3,245    | 100%  |
| library-import name (finish rule)                       | 1,353    | 41.7% |
| left as bundled (no final name, or kept the minifier's) | 1,202    | 37.0% |
| post-split reconcile (finish rule)                      | 690      | 21.3% |

### app bindings whose file could not be mapped

| name source                                                                        | bindings | share |
| ---------------------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                                              | 200,053  | 100%  |
| carried from prior by exact match [fresh text, no bodySpans]                       | 186,166  | 93.1% |
| carried from prior by close match [fresh text, no bodySpans]                       | 8,772    | 4.4%  |
| model-named [fresh text, no bodySpans]                                             | 5,027    | 2.5%  |
| left as bundled (no final name, or kept the minifier's) [fresh text, no bodySpans] | 56       | 0.0%  |
| library-import name (finish rule) [fresh text, no bodySpans]                       | 28       | 0.0%  |
| other (class-id-floor) [fresh text, no bodySpans]                                  | 2        | 0.0%  |
| left as bundled (no final name, or kept the minifier's) [generated]                | 1        | 0.0%  |
| post-split reconcile (finish rule) [generated]                                     | 1        | 0.0%  |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 699   | 148                   |
| manifest                                | 183   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 14    | 4                     |
| moved / relocated (movedPath+relocated) | 0     | 0                     |
| real: changed in place or paired        | 30    | 12                    |
| real: truly added                       | 472   | 132                   |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 197   |                       |
| = real (vendorReal)                     | 502   |                       |

### file-path stability (vendor files)

| outcome                             | files | lines charged |
| ----------------------------------- | ----- | ------------- |
| kept path, byte-identical           | 1,614 | 0             |
| new (content absent from the prior) | 33    | 472           |
| kept path, realChange               | 4     | 30            |
| kept path, nameOnly                 | 2     | 14            |

### identifier-name stability for identical code (same content signature)

| file kind: outcome       | bindings |
| ------------------------ | -------- |
| era file: different name | 3        |
| era file: same name      | 750      |
| factory file: same name  | 57,727   |
