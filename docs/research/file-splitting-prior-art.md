# Prior Art: Splitting Deobfuscated JS Into File Hierarchies

## The Problem Space

After deobfuscating/unminifying a JavaScript bundle, you have either:
1. **Webpack/Browserify**: Multiple flat files named by module ID (e.g., `1.js`, `2.js`)
2. **Rollup/esbuild (scope-hoisted)**: A single large file with all functions merged

We want to reconstruct a directory/file structure resembling a normal codebase, with two key properties:
- **Deterministic**: Same input always produces same output
- **Cross-version stable**: v1.0 and v1.1 of the same app produce similar layouts, with diffs proportional to actual code changes

## Existing Tools and Their Limitations

### Debundlers (webpack/browserify-specific)

**[debundle](https://github.com/1egoman/debundle)** - Takes webpack/browserify bundles and extracts individual modules. Limitations:
- No longer maintained (research project)
- Only works with webpack/browserify module wrappers (not scope-hoisted bundles)
- Output files are named by module ID (`1.js`, `2.js`) unless manually overridden
- Minified input produces minified output files
- "Works in a laboratory environment most of the time, but often fails on real-world bundles"
- No cross-version stability guarantees

**[webcrack](https://github.com/j4k0xb/webcrack)** - Deobfuscates, unminifies, and unpacks webpack/browserify bundles. Capabilities:
- Recovers module paths from webpack/browserify metadata when available
- Can split bundled modules back into separate files
- Active project (last updated Feb 2025)
- Does NOT handle scope-hoisted bundles (Rollup/esbuild) - these remain as a single file
- No clustering or intelligent file organization beyond what the bundler metadata provides
- No cross-version stability mechanism

### Source Map Recovery Tools

**[unwebpack-sourcemap](https://github.com/rarecoil/unwebpack-sourcemap)**, **[shuji](https://github.com/paazmaya/shuji)**, **[sourcemapper](https://github.com/tehryanx/sourcemapper)** - Reconstruct original file structure from source maps.
- Perfect reconstruction when source maps are available
- Irrelevant to our use case: we don't have source maps

### Binary Decompilers (analogous problem in different domain)

**[RetDec](https://github.com/avast/retdec)** - LLVM-based machine-code decompiler. Relevant parallels:
- Signature-based removal of statically linked library code (analogous to our library separation)
- Function recovery and reconstruction
- Deterministic output
- Does NOT attempt to reconstruct original file structure

## Academic Research: Software Module Clustering

This is the most relevant body of prior work. The academic field of "software module clustering" or "software modularization" directly addresses our problem.

### Bunch (Mancoridis et al., 1999)

**[Paper](https://www.cs.drexel.edu/~bmitchell/pubs/icsm99.pdf)** - The seminal tool in this space.

- Treats clustering as an **optimization problem** on a Module Dependency Graph (MDG)
- Uses a genetic algorithm to search the partition space
- Optimizes "Modularization Quality" (MQ): maximize intra-cluster cohesion, minimize inter-cluster coupling
- **NOT deterministic** - genetic algorithm is stochastic, different runs can produce different results
- Designed for program comprehension of existing codebases, not for splitting deobfuscated code

### Neighborhood Tree Algorithm

- Creates a tree based on neighbors in the dependency graph
- **Deterministic** - same clustering achieved across different runs
- High stability compared to search-based and hierarchical algorithms
- More relevant to our needs than Bunch due to determinism

### Hierarchical Clustering of Call Graphs (IEEE, 2019)

**[Paper](https://ieeexplore.ieee.org/document/8622426/)** - "Automatic Hierarchical Clustering of Static Call Graphs for Program Comprehension"

- Automatically constructs and visualizes static call graphs
- Clusters execution paths into hierarchical abstractions
- Labels clusters according to functional behaviors
- Produces dendrograms showing cluster relationships
- Relevant approach: hierarchical clustering naturally maps to directory hierarchies

### Graph-Based Modularization Algorithm (GMA)

**[Paper](https://www.sciencedirect.com/science/article/abs/pii/S0950584920302147)** - A graph-based clustering algorithm for software modularization.

- Specifically designed for partitioning source code into manageable modules
- Uses dependency graphs as input
- Produces module decomposition

### Microservice Extraction (Mo2oM / DEEPLY, 2025)

**[Paper](https://arxiv.org/html/2508.07486)** - Uses deep semantic embeddings + graph neural networks.

- Formulates module extraction as a soft clustering problem
- Integrates LLM-based semantic embeddings with structural dependency features
- Uses spectral clustering for flexible cluster shapes
- Most modern approach; combines structural and semantic signals
- Overkill for our use case but validates the LLM+structure hybrid approach

### Context+ (Semantic Codebase Intelligence)

**[GitHub](https://github.com/ForLoopCodes/contextplus)** - MCP server combining RAG, Tree-sitter AST, Spectral Clustering, and graph linking.

- Groups semantically related files into labeled clusters
- Uses spectral clustering on dependency graphs
- Validates that AST + clustering is a viable approach for code organization

## Community Detection Algorithms

These general-purpose graph algorithms are used by the above tools:

| Algorithm | Deterministic? | Notes |
|-----------|---------------|-------|
| Louvain | No (stochastic) | Most popular for modularity optimization, but non-deterministic |
| Leiden | No (stochastic) | Improved Louvain, still stochastic |
| Label Propagation (LPA) | No (standard) / Yes (LPA-MNI variant) | Fast but usually non-deterministic |
| Spectral Clustering | Yes (given same eigenvector computation) | Good for finding natural graph cuts |
| DBSCAN | Yes | Always same clusters for same input in same order |
| Weakly Connected Components | Yes | Too coarse for our needs |
| Neighborhood Tree | Yes | Specifically designed for software clustering |

**Key insight**: Most popular community detection algorithms are stochastic. Determinism requires either choosing a deterministic algorithm or adding deterministic tie-breaking to a stochastic one.

## Gap Analysis: What's Missing in Prior Art

No existing tool solves our full problem. Here's what's missing:

1. **Scope-hoisted bundle splitting**: webcrack and debundle only handle webpack/browserify module wrappers. Nobody splits scope-hoisted (Rollup/esbuild) bundles because the module boundaries are erased.

2. **Cross-version stability**: No tool optimizes for stability across versions. Academic clustering tools optimize for modularity quality, not for producing minimal diffs between versions.

3. **Combined structural + semantic naming**: Bunch and GMA produce numbered clusters. Nobody combines structural clustering with LLM-suggested semantic names that are cached for stability.

4. **Function-level fingerprinting for stability**: Using normalized AST hashes as stable function identifiers across versions is novel in this context. Binary analysis tools use function signatures for library detection, but not for cross-version cluster stability.

5. **Import/export reconstruction**: Debundlers reconstruct require/import statements from bundler metadata. Nobody reconstructs imports/exports from scratch based on call graph analysis of a flat file.

## Comparison: Our Spec vs Prior Art

| Aspect | Our Spec (19) | Best Prior Art |
|--------|--------------|----------------|
| Input | Any minified JS (scope-hoisted or bundled) | webcrack: webpack/browserify only |
| Determinism | Yes (sort by exactHash at every decision point) | Bunch: No; Neighborhood Tree: Yes |
| Cross-version stability | Yes (exactHash fingerprints, cluster identity by member overlap) | Nobody does this |
| Naming | LLM-suggested, cached by cluster fingerprint | Bunch: numbered; academic: behavioral labels |
| Library separation | Comment regions + webcrack metadata | webcrack: bundler metadata only |
| Clustering algorithm | Root-finding + exclusive ownership BFS | Bunch: GA optimization; Academic: various |
| Output | Real JS files with import/export statements | debundle: separate files, no import reconstruction |

## Recommendations

### What we should adopt from prior art:

1. **Modularity Quality (MQ) metric from Bunch** - Use as a diagnostic/quality metric for our clustering output, even if we don't optimize for it directly. MQ = avg(intra-cluster cohesion) - avg(inter-cluster coupling).

2. **Hierarchical clustering approach** - The academic work on hierarchical call graph clustering maps naturally to our recursive sub-splitting. Use dendrogram-style cutting to determine split thresholds.

3. **Deterministic tie-breaking** - The Neighborhood Tree algorithm's approach of deterministic neighbor-based clustering validates our choice of sorting by exactHash at every decision point.

### What we should NOT adopt:

1. **Stochastic algorithms (Louvain, Leiden, GA)** - Non-deterministic results violate our core requirement.

2. **Spectral clustering** - Adds a Python/scipy dependency for marginal benefit over our simpler BFS approach. Worth revisiting if BFS produces poor clusters on real-world code.

3. **GNN/deep learning approaches** - Overkill for our scale and adds heavy dependencies.

### What's novel in our approach:

1. **exactHash as stable function identity** - No prior work uses normalized AST hashing for cross-version cluster stability
2. **Cluster fingerprint = sorted member hashes** - Enables deterministic cluster identity and cache lookup
3. **LLM naming with fingerprint-keyed cache** - Combines semantic naming with structural stability
4. **Root-finding + exclusive ownership** - Simpler than optimization-based clustering, naturally deterministic
