# exp161-names-hop-default — 2.1.215->2.1.216 (default mode)

## Run status

| check                     | value                                                                                                                            |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| runStatus                 | MISSING (/work/exp161-hop/experiments/034-eval-harness/results/exp161-names-hop-default/2.1.216-run-status.json — card dir gone) |
| selfHopCard               | missing                                                                                                                          |
| tree 2.1.216              | present                                                                                                                          |
| tree 2.1.216-selfhop      | present                                                                                                                          |
| tree 2.1.216-selfhop-warm | present                                                                                                                          |

## Consistency checks (each counter shown able to fire)

| check                                  | value                                                                                                                                                                                                                         |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| factoryShapePairing                    | 1614/1618 fresh vendor factory files paired to a raw factory by shape (1591/1595 prior); record order would have agreed for 1453                                                                                              |
| carryOverReconstructed                 | 1595 factories inherited by (structural hash, ordinal) at the same path (the run log's "carry-over" count is the check where a log exists)                                                                                    |
| eraVendorStatements                    | 0                                                                                                                                                                                                                             |
| factoriesWithContestedClaim            | 0                                                                                                                                                                                                                             |
| indexLabelledFreshFactories            | 0                                                                                                                                                                                                                             |
| restorationApplicationSlots            | 0                                                                                                                                                                                                                             |
| restorationApplicationSlotsInFactories | 0                                                                                                                                                                                                                             |
| eraTrailFileMapping                    | 2032/2050 trails mapped to a file find their final name in that file's text (a wrong coordinate system reads far lower); 200413 fresh-text trails assigned to src/ without bodySpans because no app statement went to vendor/ |
| eraRestorationSiteAApplied             | 0                                                                                                                                                                                                                             |
| eraRestorationFrozen                   | null                                                                                                                                                                                                                          |
| mintedCensusTotal                      | 81                                                                                                                                                                                                                            |

## 1. Where code goes (the fresh tree, .humanify/ excluded)

| area    | files | lines   | bytes      | share of lines |
| ------- | ----- | ------- | ---------- | -------------- |
| TOTAL   | 6,503 | 903,201 | 47,506,312 | 100.0%         |
| src/    | 4,880 | 877,738 | 41,974,231 | 97.2%          |
| vendor/ | 1,619 | 20,468  | 5,261,327  | 2.3%           |
| other   | 4     | 4,995   | 270,754    | 0.6%           |

### vendor/ by the mechanism that placed each file (exclusive)

| mechanism                                               | files | lines  | bytes     | share of vendor bytes |
| ------------------------------------------------------- | ----- | ------ | --------- | --------------------- |
| TOTAL                                                   | 1,619 | 20,468 | 5,261,327 | 100%                  |
| inherited from prior by hash (factory carry-over)       | 1,595 | 6,712  | 4,832,477 | 91.8%                 |
| index claim moved by package                            | 0     | 0      | 0         | 0.0%                  |
| unclaimed-but-clean, moved on index owner+file (exp157) | 0     | 0      | 0         | 0.0%                  |
| whole-module factory, named this run (model/declared)   | 23    | 97     | 20,370    | 0.4%                  |
| root lib\_\* fallback                                   | 0     | 0      | 0         | 0.0%                  |
| era (app) statements filed into vendor by placement     | 0     | 0      | 0         | 0.0%                  |
| the vendor record (\_bun-modules.json)                  | 1     | 13,659 | 408,480   | 7.8%                  |
| other                                                   | 0     | 0      | 0         | 0.0%                  |

### finer detail (mechanism :: label)

| mechanism :: detail                                                                 | files | lines  | bytes     |
| ----------------------------------------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over) :: carried; origin label llm      | 1,104 | 5,072  | 3,250,530 |
| inherited from prior by hash (factory carry-over) :: carried; origin label declared | 215   | 522    | 1,220,046 |
| the vendor record (\_bun-modules.json) :: humanify's own metadata, not library code | 1     | 13,659 | 408,480   |
| inherited from prior by hash (factory carry-over) :: carried; origin label fallback | 276   | 1,118  | 361,901   |
| whole-module factory, named this run (model/declared) :: llm, decided this run      | 23    | 97     | 20,370    |

### per package (top 25 by bytes, then the long tail)

| package                             | files | lines  | bytes     | share of bytes | dominant mechanism (by bytes)                     |
| ----------------------------------- | ----- | ------ | --------- | -------------- | ------------------------------------------------- |
| (vendor root)                       | 594   | 16,228 | 2,134,737 | 40.6%          | inherited from prior by hash (factory carry-over) |
| highlight                           | 189   | 386    | 1,101,807 | 20.9%          | inherited from prior by hash (factory carry-over) |
| otlp-proto-common                   | 2     | 5      | 203,591   | 3.9%           | inherited from prior by hash (factory carry-over) |
| sharp                               | 8     | 44     | 89,276    | 1.7%           | inherited from prior by hash (factory carry-over) |
| ajv                                 | 27    | 139    | 79,510    | 1.5%           | inherited from prior by hash (factory carry-over) |
| grpc                                | 25    | 100    | 73,661    | 1.4%           | inherited from prior by hash (factory carry-over) |
| @opentelemetry/core                 | 39    | 171    | 69,092    | 1.3%           | inherited from prior by hash (factory carry-over) |
| node-forge                          | 9     | 44     | 64,104    | 1.2%           | inherited from prior by hash (factory carry-over) |
| semver                              | 25    | 134    | 60,080    | 1.1%           | inherited from prior by hash (factory carry-over) |
| google-auth-library                 | 18    | 106    | 53,495    | 1.0%           | inherited from prior by hash (factory carry-over) |
| vscode-jsonrpc                      | 7     | 72     | 45,543    | 0.9%           | inherited from prior by hash (factory carry-over) |
| @aws-sdk/core                       | 2     | 28     | 43,580    | 0.8%           | inherited from prior by hash (factory carry-over) |
| google-auth-library-iamcredentials  | 15    | 80     | 41,285    | 0.8%           | inherited from prior by hash (factory carry-over) |
| openid-client                       | 5     | 58     | 40,107    | 0.8%           | inherited from prior by hash (factory carry-over) |
| @opentelemetry/semantic-conventions | 2     | 6      | 39,059    | 0.7%           | inherited from prior by hash (factory carry-over) |
| html-entities                       | 2     | 9      | 38,819    | 0.7%           | inherited from prior by hash (factory carry-over) |
| commander                           | 8     | 51     | 37,104    | 0.7%           | inherited from prior by hash (factory carry-over) |
| xml-dom                             | 9     | 72     | 36,283    | 0.7%           | inherited from prior by hash (factory carry-over) |
| xmldom                              | 2     | 15     | 34,827    | 0.7%           | inherited from prior by hash (factory carry-over) |
| fs-extra                            | 17    | 92     | 34,433    | 0.7%           | inherited from prior by hash (factory carry-over) |
| @opentelemetry/sdk-metrics-base     | 25    | 115    | 33,909    | 0.6%           | inherited from prior by hash (factory carry-over) |
| @aws-sdk/protocol-http              | 2     | 13     | 33,454    | 0.6%           | inherited from prior by hash (factory carry-over) |
| tslib                               | 7     | 44     | 28,437    | 0.5%           | inherited from prior by hash (factory carry-over) |
| opentelemetry-core                  | 21    | 64     | 28,312    | 0.5%           | inherited from prior by hash (factory carry-over) |
| @grpc/grpc-js                       | 5     | 52     | 26,982    | 0.5%           | inherited from prior by hash (factory carry-over) |
| long tail (130 packages)            | 554   | 2,340  | 789,840   | 15.0%          | —                                                 |

### vendor-root loose files

| mechanism                                             | files | lines  | bytes     |
| ----------------------------------------------------- | ----- | ------ | --------- |
| inherited from prior by hash (factory carry-over)     | 578   | 2,521  | 1,714,683 |
| the vendor record (\_bun-modules.json)                | 1     | 13,659 | 408,480   |
| whole-module factory, named this run (model/declared) | 15    | 48     | 11,574    |

## 2. How identifiers are named

### vendor/ factory bodies (each binding zipped against the raw bundle's factory)

| name source                                                         | bindings | share |
| ------------------------------------------------------------------- | -------- | ----- |
| TOTAL                                                               | 53,160   | 100%  |
| left as bundled (minifier's name)                                   | 34,742   | 65.4% |
| carried from prior: file inherited, still the prior's minifier name | 17,949   | 33.8% |
| factory not paired to the raw bundle                                | 469      | 0.9%  |

### vendor/ era (app) statements (strategy trail, by terminal step)

| name source | bindings | share |
| ----------- | -------- | ----- |
| TOTAL       | 0        | 100%  |

### src/ (strategy trail, by terminal step) — the contrast

| name source                                             | bindings | share |
| ------------------------------------------------------- | -------- | ----- |
| TOTAL                                                   | 203,581  | 100%  |
| carried from prior by exact match                       | 186,618  | 91.7% |
| carried from prior by close match                       | 8,773    | 4.3%  |
| model-named                                             | 4,992    | 2.5%  |
| library-import name (finish rule)                       | 1,381    | 0.7%  |
| left as bundled (no final name, or kept the minifier's) | 1,118    | 0.5%  |
| post-split reconcile (finish rule)                      | 697      | 0.3%  |
| other (class-id-floor)                                  | 2        | 0.0%  |

### app bindings whose file could not be mapped

| name source                                                         | bindings | share  |
| ------------------------------------------------------------------- | -------- | ------ |
| TOTAL                                                               | 2        | 100%   |
| left as bundled (no final name, or kept the minifier's) [generated] | 2        | 100.0% |

## 3. vendor/ churn, prior → fresh (the eval's vendor scorer)

| bucket                                  | lines | of which mint-bearing |
| --------------------------------------- | ----- | --------------------- |
| TOTAL churnLines                        | 303   | 0                     |
| manifest                                | 183   | 0                     |
| name re-rolls (nameOnly+freeReroll)     | 0     | 0                     |
| moved / relocated (movedPath+relocated) | 0     | 0                     |
| real: changed in place or paired        | 0     | 0                     |
| real: truly added                       | 120   | 0                     |
| real: truly removed                     | 0     | 0                     |
| other (unparsed)                        | 0     | 0                     |
| = noise (vendorLn)                      | 183   |                       |
| = real (vendorReal)                     | 120   |                       |

### file-path stability (vendor files)

| outcome                             | files | lines charged |
| ----------------------------------- | ----- | ------------- |
| kept path, byte-identical           | 1,595 | 0             |
| new (content absent from the prior) | 23    | 120           |

### identifier-name stability for identical code (same content signature)

| file kind: outcome      | bindings |
| ----------------------- | -------- |
| factory file: same name | 57,727   |
