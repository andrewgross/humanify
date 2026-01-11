# CLI Design

## Overview

Unified CLI that works with any OpenAI-compatible LLM provider. Replaces the current separate `local`, `openai`, `gemini` commands with a single `humanify` command.

## Command Structure

```
humanify <input> [options]
humanify analyze <input> [options]
humanify cache <subcommand>
humanify download <model>
```

## Main Command

```bash
humanify <input> [options]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `input`  | Path to minified JS file or directory |

### LLM Options

| Option | Env Variable | Default | Description |
|--------|--------------|---------|-------------|
| `--endpoint <url>` | `HUMANIFY_ENDPOINT` | `https://api.openai.com/v1` | OpenAI-compatible API endpoint |
| `--api-key <key>` | `HUMANIFY_API_KEY` | - | API key for the endpoint |
| `--model <name>` | `HUMANIFY_MODEL` | `gpt-4o-mini` | Model identifier |
| `--local` | - | - | Use local llama.cpp model |
| `--local-model <path>` | `HUMANIFY_LOCAL_MODEL` | `~/.humanify/models/phi-3-mini.gguf` | Path to local model file |

### Output Options

| Option | Default | Description |
|--------|---------|-------------|
| `-o, --output <path>` | `./humanified/` | Output directory or file |
| `--source-map-only` | false | Only generate source map, don't rewrite code |
| `--inline-source-map` | false | Embed source map in output file |
| `--no-source-map` | false | Don't generate source map |

### Processing Options

| Option | Default | Description |
|--------|---------|-------------|
| `--concurrency <n>` | 10 | Max parallel LLM calls |
| `--cache <path>` | `~/.humanify/cache` | Cache directory |
| `--no-cache` | false | Disable caching |
| `--no-skip-libraries` | false | Process library code too |
| `--include <glob>` | - | Only process matching files |
| `--exclude <glob>` | - | Skip matching files |

### Other Options

| Option | Description |
|--------|-------------|
| `-v, --verbose` | Show detailed progress |
| `--dry-run` | Show what would be processed without doing it |
| `--cost-estimate` | Show estimated API cost before processing |
| `-h, --help` | Show help |
| `--version` | Show version |

## Examples

### Basic Usage

```bash
# OpenAI (uses OPENAI_API_KEY env var)
humanify bundle.min.js -o humanified/

# Explicit OpenAI config
humanify bundle.min.js \
  --endpoint https://api.openai.com/v1 \
  --model gpt-4o-mini \
  --api-key sk-xxx \
  -o humanified/
```

### Alternative Providers

```bash
# OpenRouter (access Claude, Llama, etc.)
humanify bundle.min.js \
  --endpoint https://openrouter.ai/api/v1 \
  --model anthropic/claude-3-haiku \
  --api-key $OPENROUTER_API_KEY

# Together AI
humanify bundle.min.js \
  --endpoint https://api.together.xyz/v1 \
  --model meta-llama/Llama-3-70b-chat-hf \
  --api-key $TOGETHER_API_KEY

# Local vLLM server
humanify bundle.min.js \
  --endpoint http://localhost:8000/v1 \
  --model local-model \
  --api-key none

# Ollama
humanify bundle.min.js \
  --endpoint http://localhost:11434/v1 \
  --model llama3.1 \
  --api-key ollama
```

### Local Model

```bash
# Use bundled local model
humanify bundle.min.js --local

# Use specific local model
humanify bundle.min.js --local --local-model ~/models/codellama-7b.gguf

# Download recommended local model first
humanify download phi-3-mini
humanify bundle.min.js --local
```

### Source Map Options

```bash
# Default: code + external .map file
humanify bundle.min.js -o output.js
# Creates: output.js, output.js.map

# Inline source map (single file)
humanify bundle.min.js --inline-source-map -o output.js

# Source map only (don't rewrite code)
humanify bundle.min.js --source-map-only
# Creates: bundle.min.js.map
```

### Caching

```bash
# Use default cache
humanify bundle.min.js -o output/

# Custom cache location
humanify bundle.min.js --cache ./my-cache -o output/

# Disable cache
humanify bundle.min.js --no-cache -o output/

# Cross-version caching
humanify v1/bundle.min.js --cache ./project-cache -o v1/
humanify v2/bundle.min.js --cache ./project-cache -o v2/
# v2 reuses cached results from v1 where functions match
```

### Filtering

```bash
# Only process src/ files
humanify bundle.min.js --include "src/**" -o output/

# Skip vendor code
humanify bundle.min.js --exclude "vendor/**,node_modules/**" -o output/

# Dry run to see what would be processed
humanify bundle.min.js --dry-run --verbose
```

## Analyze Command

Analyze a bundle without processing - useful for understanding structure before committing to LLM costs.

```bash
humanify analyze <input> [options]
```

### Options

| Option | Description |
|--------|-------------|
| `-o, --output <path>` | Write analysis to JSON file |
| `--format <fmt>` | Output format: `json`, `text` (default: text) |

### Example

```bash
$ humanify analyze bundle.min.js

Bundle Analysis: bundle.min.js
==============================

Files: 45
  - Library: 12 (react, lodash, moment)
  - Novel: 33

Functions: 1,847
  - Library: 1,659 (will be skipped)
  - Novel: 188 (will be processed)

Estimated Processing:
  - LLM calls: ~564 (188 functions × ~3 identifiers each)
  - Tokens: ~282,000
  - Cost (gpt-4o-mini): ~$0.14
  - Time: ~2-3 minutes at concurrency 10

Detected Libraries:
  - react (847 functions)
  - lodash (412 functions)
  - moment (400 functions)
```

## Cache Command

Manage the humanification cache.

```bash
humanify cache <subcommand>
```

### Subcommands

```bash
# Show cache statistics
humanify cache stats
# Output:
# Cache: ~/.humanify/cache
# Entries: 12,847
# Size: 45.2 MB
# Hit rate: 89% (last session)

# Clear entire cache
humanify cache clear

# Clear old entries (keep last 30 days)
humanify cache prune --older-than 30d

# Export cache for sharing
humanify cache export -o project-cache.json

# Import shared cache
humanify cache import project-cache.json
```

## Download Command

Download local models.

```bash
humanify download <model>
```

### Available Models

| Model | Size | Description |
|-------|------|-------------|
| `phi-3-mini` | 2.4 GB | Fast, good for simple renames (default) |
| `llama-3-8b` | 4.7 GB | Better quality, slower |
| `codellama-7b` | 3.8 GB | Optimized for code |

### Example

```bash
$ humanify download phi-3-mini

Downloading phi-3-mini (2.4 GB)...
[████████████████████████████████████████] 100%

Model saved to: ~/.humanify/models/phi-3-mini.gguf

To use: humanify bundle.min.js --local
```

## Configuration File

Support a config file for project-specific defaults:

```yaml
# .humanifyrc.yaml or humanify.config.yaml

# LLM settings
endpoint: https://api.openai.com/v1
model: gpt-4o-mini
# api-key: loaded from HUMANIFY_API_KEY env var

# Processing
concurrency: 15
cache: ./.humanify-cache

# Output
output: ./src/deobfuscated
inlineSourceMap: true

# Filtering
exclude:
  - "vendor/**"
  - "**/*.test.js"
```

Load order:
1. Default values
2. Config file (if present)
3. Environment variables
4. CLI arguments (highest priority)

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Invalid arguments |
| 3 | Input file not found |
| 4 | LLM API error |
| 5 | Parse error (invalid JavaScript) |

## Progress Output

### Default Progress (non-verbose)

Single-line progress that updates in place:

```bash
$ humanify bundle.min.js -o output/

[42%] 78/188 functions | LLM: 5 in-flight | ETA: 1m 02s
```

### Verbose Progress (--verbose)

Detailed multi-line output showing all stages:

```bash
$ humanify bundle.min.js -o output/ --verbose

Unpacking bundle.min.js...
  ✓ Extracted 45 files

Analyzing...
  ✓ Found 1,847 functions
  ✓ Detected libraries: react, lodash, moment
  ✓ Skipping 1,659 library functions
  ✓ Processing 188 novel functions

Cache lookup...
  ✓ Cache hits: 142/188 (75%)
  ✓ Need to process: 46 functions

Processing [████████████████░░░░░░░░░░░░░░░░░░░░░░░░] 42%
  Functions: 78/188 done | 5 processing | 12 ready | 93 pending
  LLM Calls: 156 done | 5 in-flight | 0 failed | avg 287ms
  Elapsed: 45s | ETA: 1m 02s

Writing output...
  ✓ output/src/components/UserProfile.js
  ✓ output/src/components/UserProfile.js.map
  ...

Done!
  Files: 33
  Functions renamed: 188
  LLM calls: 312 (avg 287ms)
  Time: 1m 47s
  Cache: 142 hits, 46 new entries saved
```

### Progress Metrics Explained

| Field | Description |
|-------|-------------|
| `Functions: X/Y done` | Completed functions out of total |
| `Z processing` | Functions currently being processed (limited by concurrency) |
| `W ready` | Functions whose dependencies are met, waiting for a slot |
| `P pending` | Functions still waiting for dependencies to complete |
| `LLM Calls: N done` | Total successful LLM API calls |
| `M in-flight` | Currently active LLM requests |
| `F failed` | Requests that failed after all retries |
| `avg Xms` | Average LLM response time |
| `ETA` | Estimated time remaining based on current rate |

### Progress Display Modes

```bash
# Default: compact single-line (updates in place)
humanify bundle.min.js -o output/

# Verbose: detailed multi-line
humanify bundle.min.js -o output/ --verbose

# Quiet: no progress, only errors and final summary
humanify bundle.min.js -o output/ --quiet

# JSON progress (for tooling integration)
humanify bundle.min.js -o output/ --progress-json
# Outputs NDJSON: {"type":"progress","functions":{"done":78,"total":188},...}
```

### Interpreting Progress for Bottlenecks

**High `in-flight` but slow progress:**
- API is slow or rate-limited
- Consider reducing `--concurrency`

**Low `in-flight` but high `ready`:**
- Concurrency too low, increase `--concurrency`

**High `pending`, low `ready`:**
- Dependency chain is deep
- Normal for deeply nested code

**High `failed` count:**
- API errors or quota issues
- Check API key and rate limits

## Environment Variables

| Variable | Description |
|----------|-------------|
| `HUMANIFY_ENDPOINT` | Default API endpoint |
| `HUMANIFY_API_KEY` | API key (also supports `OPENAI_API_KEY`) |
| `HUMANIFY_MODEL` | Default model |
| `HUMANIFY_CACHE` | Cache directory |
| `HUMANIFY_CONCURRENCY` | Default concurrency |
| `NO_COLOR` | Disable colored output |
| `DEBUG` | Enable debug logging |
