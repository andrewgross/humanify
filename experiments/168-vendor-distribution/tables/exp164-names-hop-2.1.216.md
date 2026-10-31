# exp164-names-hop — 2.1.215->2.1.216 (names mode)

## Run status

| check                     | value                |
| ------------------------- | -------------------- |
| runStatus                 | exitCode 0, errors 0 |
| selfHopCard               | present              |
| tree 2.1.216              | present              |
| tree 2.1.216-selfhop      | present              |
| tree 2.1.216-selfhop-warm | present              |

## Consistency checks (each counter shown able to fire)

| check                                  | value                                                                                                                                                                                                                        |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| factoryShapePairing                    | 1578/1618 fresh vendor factory files paired to a raw factory by shape (1555/1595 prior); record order would have agreed for 1417                                                                                             |
| carryOverReconstructed                 | 1595 factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)                                                                                   |
| eraVendorStatements                    | 103                                                                                                                                                                                                                          |
| factoriesWithContestedClaim            | 6                                                                                                                                                                                                                            |
| indexLabelledFreshFactories            | 1383                                                                                                                                                                                                                         |
| restorationApplicationSlots            | 105383                                                                                                                                                                                                                       |
| restorationApplicationSlotsInFactories | 61924                                                                                                                                                                                                                        |
| eraTrailFileMapping                    | 185860/199916 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 0 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                            |
| eraRestorationFrozen                   | 133                                                                                                                                                                                                                          |
| mintedCensusTotal                      | 501                                                                                                                                                                                                                          |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 903,201 | 48,165,430 | 100.0%         |
| src/    | 4,845 | 874,152 | 41,726,511 | 96.8%          |
| vendor/ | 1,654 | 24,054  | 6,172,122  | 2.7%           |
| other   | 4     | 4,995   | 266,797    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,654 | 24,054 | 6,172,122 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,595 | 6,712  | 5,581,706 | 90.4%                 |
| index claim moved by package                            | 18    | 486    | 22,353    | 0.4%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 22    | 92     | 20,508    | 0.3%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 18    | 3,105  | 105,255   | 1.7%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 13,659 | 442,300   | 7.2%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                                                                     | files | lines  | bytes     |
| --------------------------------------------------------------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: claimed                            | 797   | 4,017  | 3,699,126 |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: unclaimed (exp157 rule)            | 546   | 1,500  | 1,066,210 |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm                                                          | 209   | 921    | 517,882   |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code                                                     | 1     | 13,659 | 442,300   |
| inherited from prior by hash (factory carry-over) :: carried; origin label index; this run's claims: claim unknown (factory not paired) | 39    | 257    | 278,100   |
| era (app) statements filed into vendor by placement :: era file: fossil:unique-signature+prior-path-by-hash, fossil:unique-signature    | 9     | 2,837  | 93,110    |
| whole-module factory, named this run (model/declared) :: llm, decided this run                                                          | 22    | 92     | 20,508    |
| index claim moved by package :: era file: library-index:claimed                                                                         | 17    | 481    | 20,387    |
| inherited from prior by hash (factory carry-over) :: carried; origin label declared                                                     | 2     | 13     | 19,458    |
| era (app) statements filed into vendor by placement :: era file: fossil:unique-signature, fossil:unique-signature+prior-path-by-hash    | 3     | 192    | 8,772     |
| era (app) statements filed into vendor by placement :: era file: fossil:edge-corroborated                                               | 3     | 45     | 2,091     |
| index claim moved by package :: index, decided this run, claimed                                                                        | 1     | 5      | 1,966     |
| era (app) statements filed into vendor by placement :: era file: fossil:high-overlap-unique                                             | 3     | 31     | 1,282     |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback                                                     | 2     | 4      | 930       |

### per package (top 25 by bytes, then the long tail)

| package                             | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                       |
| ----------------------------------- | ----- | ------ | --------- | -------------- | --------------------------------------------------- |
| highlight.js                        | 193   | 397    | 1,137,114 | 18.4%          | inherited from prior by hash (factory carry-over)   |
| (vendor root)                       | 3     | 13,663 | 443,230   | 7.2%           | the vendor record (\_bun-modules.json)              |
| @grpc/grpc-js                       | 65    | 407    | 373,839   | 6.1%           | inherited from prior by hash (factory carry-over)   |
| node-forge                          | 41    | 292    | 355,995   | 5.8%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/otlp-transformer     | 20    | 73     | 246,841   | 4.0%           | inherited from prior by hash (factory carry-over)   |
| @mixmark-io/domino                  | 16    | 99     | 220,885   | 3.6%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/sdk-metrics          | 99    | 445    | 182,105   | 3.0%           | inherited from prior by hash (factory carry-over)   |
| acorn                               | 1     | 10     | 152,019   | 2.5%           | inherited from prior by hash (factory carry-over)   |
| react                               | 3     | 19     | 135,852   | 2.2%           | inherited from prior by hash (factory carry-over)   |
| google-auth-library                 | 43    | 217    | 132,606   | 2.1%           | inherited from prior by hash (factory carry-over)   |
| @xmldom/xmldom                      | 8     | 38     | 114,802   | 1.9%           | inherited from prior by hash (factory carry-over)   |
| protobufjs                          | 29    | 168    | 114,771   | 1.9%           | inherited from prior by hash (factory carry-over)   |
| semver                              | 113   | 500    | 113,069   | 1.8%           | inherited from prior by hash (factory carry-over)   |
| @aws-sdk/core                       | 4     | 46     | 105,021   | 1.7%           | inherited from prior by hash (factory carry-over)   |
| sharp                               | 15    | 103    | 91,694    | 1.5%           | inherited from prior by hash (factory carry-over)   |
| xmlbuilder                          | 33    | 177    | 89,832    | 1.5%           | inherited from prior by hash (factory carry-over)   |
| @smithy/core                        | 6     | 40     | 88,601    | 1.4%           | inherited from prior by hash (factory carry-over)   |
| openid-client                       | 28    | 143    | 79,029    | 1.3%           | inherited from prior by hash (factory carry-over)   |
| vscode-jsonrpc                      | 14    | 93     | 76,511    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/core                 | 63    | 229    | 75,544    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| fs-extra                            | 54    | 284    | 74,232    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| @opentelemetry/semantic-conventions | 6     | 14     | 72,345    | 1.2%           | inherited from prior by hash (factory carry-over)   |
| jose                                | 4     | 2,072  | 64,678    | 1.0%           | era (app) statements filed into vendor by placement |
| fast-xml-parser                     | 1     | 8      | 50,461    | 0.8%           | inherited from prior by hash (factory carry-over)   |
| commander                           | 8     | 51     | 49,356    | 0.8%           | inherited from prior by hash (factory carry-over)   |
| long tail (300 packages)            | 784   | 4,466  | 1,531,690 | 24.8%          | —                                                   |

### vendor-root loose files

| mechanism                                         | files | lines  | bytes   |
| ------------------------------------------------- | ----- | ------ | ------- |
| the vendor record (\_bun-modules.json)            | 1     | 13,659 | 442,300 |
| inherited from prior by hash (factory carry-over) | 2     | 4      | 930     |

## 2. How identifiers are named

### vendor/ factory bodies (each binding zipped against the raw bundle's factory)

| name source                                                                          | bindings | share |
| ------------------------------------------------------------------------------------ | -------- | ----- |
| TOTAL                                                                                | 53,160   | 100%  |
| restored from the library index (file answered this run; bytes inherited from prior) | 27,572   | 51.9% |
| left as bundled (minifier's name)                                                    | 11,084   | 20.9% |
| carried from prior: file inherited, still the prior's minifier name                  | 10,602   | 19.9% |
| factory not paired to the raw bundle                                                 | 3,792    | 7.1%  |
| restored from the library index (vendored body)                                      | 110      | 0.2%  |

### vendor/ era (app) statements (strategy trail, by terminal step)

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 496      | 100%  |
| carried from prior by exact match                       | 339      | 68.3% |
| left as bundled (no final name, or kept the minifier's) | 89       | 17.9% |
| post-split reconcile (finish rule)                      | 42       | 8.5%  |
| model-named                                             | 26       | 5.2%  |

### src/ (strategy trail, by terminal step) — the contrast

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 200,798  | 100%  |
| carried from prior by exact match                       | 183,751  | 91.5% |
| carried from prior by close match                       | 8,730    | 4.3%  |
| model-named                                             | 4,957    | 2.5%  |
| library-import name (finish rule)                       | 1,381    | 0.7%  |
| left as bundled (no final name, or kept the minifier's) | 1,296    | 0.6%  |
| post-split reconcile (finish rule)                      | 681      | 0.3%  |
| other (class-id-floor)                                  | 2        | 0.0%  |

### app bindings whose file could not be mapped

| name source                                                                        | bindings | share |
| ---------------------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                                              | 2,190    | 100%  |
| carried from prior by exact match [fresh text, no bodySpans]                       | 2,052    | 93.7% |
| model-named [reconciled]                                                           | 52       | 2.4%  |
| model-named [fresh text, no bodySpans]                                             | 45       | 2.1%  |
| carried from prior by close match [fresh text, no bodySpans]                       | 34       | 1.6%  |
| left as bundled (no final name, or kept the minifier's) [generated]                | 4        | 0.2%  |
| left as bundled (no final name, or kept the minifier's) [fresh text, no bodySpans] | 2        | 0.1%  |
| post-split reconcile (finish rule) [generated]                                     | 1        | 0.0%  |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 761   | 122                   |
| manifest                                | 183   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 72    | 16                    |
| moved / relocated (movedPath+relocated) | 0     | 0                     |
| real: changed in place or paired        | 34    | 8                     |
| real: truly added                       | 472   | 98                    |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 255   |                       |
| = real (vendorReal)                     | 506   |                       |

### file-path stability (vendor files)

| outcome                             | files | lines charged |
| ----------------------------------- | ----- | ------------- |
| kept path, byte-identical           | 1,612 | 0             |
| new (content absent from the prior) | 33    | 472           |
| kept path, realChange               | 5     | 34            |
| kept path, nameOnly                 | 3     | 72            |

### identifier-name stability for identical code (same content signature)

| file kind: outcome       | bindings |
| ------------------------ | -------- |
| era file: different name | 12       |
| era file: same name      | 728      |
| factory file: same name  | 57,727   |
