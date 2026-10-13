#!/usr/bin/env bash
#
# Eval harness driver — score the CURRENT pipeline on the configured version
# pairs and store the result under a MODEL label so runs stack up side by side.
#
#   npm run eval -- score <model-label> [flags]
#   (which runs: experiments/034-eval-harness/run.sh <run-config.json>)
#
# ONE RUN CONFIG (config sweep step 7, 2026-10-13). run.sh parses NO flags:
# `npm run eval -- score` (scripts/eval.ts) validates every flag once,
# resolves every default, and writes one run config JSON
# (<workdir>/<label>.run-config.json); this script takes only its path and
# reads every setting from it. Its sha256 is recorded in the label's
# pipeline.json. The flags themselves — what each means, the base modes
# (scratch default / --seeded-base / --archive-prior), --pairs, --endpoint,
# --llm-cache, --no-layout, --no-vendor-score, --no-boot-prompt,
# --no-self-hop, --inputs-base, --priors-base, --workdir, --bin,
# --force-mixed, --pipeline-set — are documented where they are parsed:
# SCORE_FLAGS and ScoreRunConfig in scripts/eval.ts.
#
# --bin (config field `bin`) runs at ALL THREE launch sites (rebase, scored
# leg, self-hop). The harness BUILDS it first (`cargo build --release
# --locked` in the cargo workspace owning it) and REFUSES a binary whose
# build commit is not this label's commit (experiments/lib/pipeline-bin.ts)
# unless forceMixed. There is no other pipeline: the TS program was deleted
# at the cutover (docs/rust-port/19-cutover.md).
#
# HOW A LAUNCH IS CONFIGURED (config sweep step 5, 2026-10-13): the binary
# takes only its operational flags (input, -o, --prior-version, --endpoint,
# --model, --config, -v); every other setting goes in a TOML file written
# per launch by experiments/lib/pipeline-config.ts, next to the launch's
# output, and passed as --config.
#
# The self-hop always runs both halves (the 5b self-hop gate, 00-control §3):
# the cold leg records into a fresh cache dir, then a warm leg replays a
# scratch COPY of it and must be byte-identical with 0 writes.
#
# <model-label> names this run (a branch, a commit, an idea — e.g.
# "main-4117212" or "fix-close-match"). Results land in results/<model-label>/;
# `leaderboard.ts` then compares every model. Re-running a label overwrites it.
#
# One pipeline run per pair (~10-15 min each); a failed pair is logged and
# skipped, never aborts the sweep. Deterministic metrics are stable run-to-run;
# the naming-noise magnitude carries the LLM floor (see README).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
CFG="$HERE/pairs.json"

RUN_CONFIG="${1:-}"
if [[ $# -ne 1 || "$RUN_CONFIG" == --* || ! -f "$RUN_CONFIG" ]]; then
  echo "usage: run.sh <run-config.json> — the run config \`npm run eval -- score <label> [flags]\` writes; run.sh parses no flags (config sweep step 7)" >&2
  exit 2
fi
command -v jq >/dev/null || { echo "jq required"; exit 1; }
[[ "$(jq -r .kind "$RUN_CONFIG")" == "eval-score" ]] || {
  echo "run.sh: $RUN_CONFIG is not an eval-score run config" >&2; exit 2; }
# run_config <jq filter>: one field of the run config ("" for null).
run_config() { jq -r "$1 // empty" "$RUN_CONFIG"; }
# run_config_flag <jq filter>: a boolean field as 1/0.
run_config_flag() { jq -r "if $1 then 1 else 0 end" "$RUN_CONFIG"; }
MODEL=$(run_config .label)
WORK=$(run_config .workdir)
BASE_MODE=$(run_config .baseMode)
PAIRS_FILTER=$(run_config .pairs)
ENDPOINT_OVERRIDE=$(run_config .endpoint)
LLM_CACHE_DIR=$(run_config .llmCache)
RUN_LAYOUT=$(run_config_flag .layout)
RUN_VENDOR=$(run_config_flag .vendorScore)
RUN_BOOT_PROMPT=$(run_config_flag .bootPrompt)
RUN_SELF_HOP=$(run_config_flag .selfHop)
INPUTS=$(run_config .inputsBase)
PRIORS=$(run_config .priorsBase)
BIN=$(run_config .bin)
FORCE_MIXED=$(run_config_flag .forceMixed)
PIPELINE_SETS=()
while IFS= read -r entry; do
  PIPELINE_SETS+=("$entry")
done < <(jq -r '.pipelineSettings[]' "$RUN_CONFIG")
# The run config's sha256, for pipeline.json (sha256sum on Linux, shasum on macOS).
if command -v sha256sum >/dev/null; then
  RUN_CONFIG_SHA=$(sha256sum "$RUN_CONFIG" | cut -d' ' -f1)
else
  RUN_CONFIG_SHA=$(shasum -a 256 "$RUN_CONFIG" | cut -d' ' -f1)
fi
# The LLM server's address is local, untracked configuration (the repo is
# public): --endpoint, else .humanify.local.json. Resolved before anything is
# built, so a missing address costs nothing.
source "$REPO/experiments/lib/llm-endpoint.sh"
ENDPOINT=$(resolve_llm_endpoint "$ENDPOINT_OVERRIDE" "$REPO") || exit 2
# The Node heap of the two ANALYSIS steps (analyze.ts, trail-report.ts) — the
# only Node processes here. The pipeline is a Rust binary and gets no
# NODE_OPTIONS (the inert --heap-mb flag that set one was deleted, config
# sweep 2026-10-13). Sized for the biggest pair; a ceiling, not a reservation.
ANALYSIS_HEAP_MB=65536
# Fatal when bun is missing, before any pair runs — a sweep that cannot boot its
# output is not a gated sweep.
source "$REPO/experiments/lib/boot-gate.sh"

# WHAT RUNS THE PIPELINE — the Rust binary, one array used at all three launch
# sites (rebase, scored leg via run-pipeline.ts, self-hop). run-launch.test.ts
# holds the launches byte-identical to the golden the binary-scored references
# ran with. Built HERE, so the label's commit is the binary's commit by
# construction — or refused (exit 2) before anything launches.
LABEL_COMMIT=$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || true)
echo "=== --bin: building and recording $BIN ==="
FORCE_ARGS=()
[[ "$FORCE_MIXED" == "1" ]] && FORCE_ARGS=(--force-mixed)
BIN_JSON=$(npx tsx "$REPO/experiments/lib/pipeline-bin.ts" "$BIN" "$LABEL_COMMIT" \
  ${FORCE_ARGS[@]+"${FORCE_ARGS[@]}"})
BIN_RC=$?
if [[ $BIN_RC -eq 2 ]]; then exit 2; fi
if [[ $BIN_RC -ne 0 || -z "$BIN_JSON" ]]; then
  echo "FATAL: could not build/record --bin $BIN (exit $BIN_RC)" >&2
  exit 1
fi
BIN=$(jq -r .path <<< "$BIN_JSON")
PIPE_CMD=("$BIN")
echo "PIPELINE: Rust binary $BIN"
echo "  sha256 $(jq -r .sha256 <<< "$BIN_JSON"), built from $(jq -r '.commit | if . == "" then "an UNKNOWN commit" else .[0:12] end' <<< "$BIN_JSON")$(jq -r 'if .dirty then " (DIRTY tree)" else "" end' <<< "$BIN_JSON")"
RUNCFG_EXTRA=$(jq -cn --arg bin "$BIN" --argjson rec "$BIN_JSON" \
  '{command:[$bin], bin:$rec}')
# The extra pipeline settings, as JSON (null when none: the default records
# stay byte-identical to the golden).
PIPELINE_SETS_JSON=null
if [[ ${#PIPELINE_SETS[@]} -gt 0 ]]; then
  PIPELINE_SETS_JSON=$(printf '%s\n' "${PIPELINE_SETS[@]}" | jq -R . | jq -cs .)
  echo "PIPELINE SETTINGS (every launch): ${PIPELINE_SETS[*]}"
fi

case "$BASE_MODE" in
  scratch) echo "BASE: scratch — each v-1 rebuilt by this pipeline with NO prior (a full cold run per base)" ;;
  seeded)  echo "BASE: seeded (--seeded-base) — each v-1 rebuilt with the ARCHIVE as --prior-version; it inherits the archive's names. Comparable only to seeded labels." ;;
  archive) echo "BASE: archive (--archive-prior) — no rebuild; scored against the archive tree (~3.7x worse)" ;;
esac

RESULTS="$HERE/results/$MODEL"
mkdir -p "$RESULTS" "$WORK"
# The label's record: the binary, the extra settings, the base mode, the run
# config this run read (path + sha256), and the boot gate's pinned model.
jq -cn --argjson bin "$BIN_JSON" --argjson extra "$PIPELINE_SETS_JSON" \
  --arg baseMode "$BASE_MODE" --arg cfgPath "$RUN_CONFIG" \
  --arg cfgSha "$RUN_CONFIG_SHA" --arg bootModel "$BOOT_GATE_MODEL" \
  '{pipeline:({kind:"rust-bin", bin:$bin}
    + (if $extra == null then {} else {pipelineSettings:$extra} end)),
    baseMode:$baseMode, runConfig:{path:$cfgPath, sha256:$cfgSha},
    bootGateModel:$bootModel}' > "$RESULTS/pipeline.json"

# pairs.json carries the laptop's absolute paths, and the bahadur devcontainer
# deliberately mirrors them, so the fixture roots resolve unchanged in both
# places; eval.ts resolved inputsBase / priorsBase (its --inputs-base /
# --priors-base, else pairs.json) into the run config.

# LLM response cache: OFF by default, because it MASKS the model's inherent
# variance. A gate run has to reproduce what a real user sees -- every prompt
# live, from scratch -- and exp047's first gate was accidentally cache-pinned:
# all 24,079 entries pre-dated the run and not one new entry was written, so no
# prompt reached the model at all. The KPIs it produced were replayed answers.
#
# Pass --llm-cache <dir> to opt IN, for fast iteration or for probing a
# deterministic surface that does not depend on LLM output. Anything whose
# numbers go into a RESULTS.md or a ship/no-ship gate must run without it.
if [[ -n "$LLM_CACHE_DIR" ]]; then
  echo "LLM CACHE: ON ($LLM_CACHE_DIR) -- NOT valid for a gate run"
else
  # The env fallback for the cache flag was deleted 2026-08-12 (flags only),
  # so omitting the key IS enough now.
  echo "LLM CACHE: OFF (cold, every prompt live) -- gate-valid"
fi
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
# The binary reads its API key from HUMANIFY_API_KEY only (config sweep
# step 4, 2026-10-13: `--api-key` was retired — a secret never goes on a
# command line, which the launch record would keep). Exported once; every
# launch below inherits it.
export HUMANIFY_API_KEY="$APIKEY"
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

# pipeline_config <out.toml> <section.key=value>...: write one launch's file.
# The harness's own keys are listed in scripts/eval.ts (SCORE_HARNESS_KEYS),
# which refuses a --pipeline-set naming one before anything builds.
pipeline_config() {
  npx tsx "$REPO/experiments/lib/pipeline-config.ts" "$1" \
    "llm.reasoning_effort=$EFFORT" "llm.concurrency=$CONC" "split.enabled=true" \
    "${@:2}" ${PIPELINE_SETS[@]+"${PIPELINE_SETS[@]}"}
}
CACHE_SETS=()
[[ -n "$LLM_CACHE_DIR" ]] && CACHE_SETS=("llm.cache_dir=$LLM_CACHE_DIR")

# Record what produced this model, for provenance.
git -C "$REPO" rev-parse --short HEAD > "$RESULTS/commit.txt" 2>/dev/null || true

npairs=$(jq '.pairs | length' "$CFG")
for i in $(seq 0 $((npairs - 1))); do
  FROM=$(jq -r ".pairs[$i].from" "$CFG")
  TO=$(jq -r ".pairs[$i].to" "$CFG")
  PAIR="$FROM->$TO"

  # --pairs "215->216,85->86" restricts the sweep to the named pairs, so a
  # targeted probe costs one run instead of the full ~1hr sweep. Either the full
  # form ("2.1.215->2.1.216") or the patch-only shorthand ("215->216") matches.
  if [[ -n "$PAIRS_FILTER" ]]; then
    SHORT="${FROM##*.}->${TO##*.}"
    if [[ ",$PAIRS_FILTER," != *",$PAIR,"* && ",$PAIRS_FILTER," != *",$SHORT,"* ]]; then
      echo "SKIP $PAIR (not in --pairs)"
      continue
    fi
  fi

  INPUT="$INPUTS/claude-code-$TO/binary-decompiled/src/entrypoints/index.js"
  PRIOR="$PRIORS/claude-code-$FROM/.humanify/humanified.js"
  PRIOR_LEDGER="$PRIORS/claude-code-$FROM/.humanify/split-ledger.json"
  OUT="$WORK/$MODEL/$TO"
  STATS="$RESULTS/$TO.stats.json"
  LOG="$RESULTS/$TO.log"

  if [[ ! -f "$INPUT" ]]; then echo "SKIP $PAIR (no input $INPUT)"; continue; fi
  if [[ ! -f "$PRIOR" ]]; then echo "SKIP $PAIR (no prior $PRIOR)"; continue; fi
  # The self-hop re-runs the LAST pair this sweep attempted. It used to read
  # the loop variables after the loop — pairs.json's last TO even when
  # `--pairs` had skipped it — so a subset run silently self-hopped nothing.
  LAST_TO="$TO"
  LAST_INPUT="$INPUT"

  # The base (header, BASE MODE). scratch (the DEFAULT) rebuilds v-1 with the
  # CURRENT pipeline and NO prior, so both sides of the pair are pure
  # current-pipeline output — a full cold run per base. seeded rebuilds it
  # with the ARCHIVE as --prior-version (it inherits the archive's names by
  # exact match: the protocol of every reference before 2026-10-03). archive
  # skips the rebuild and scores against the archive tree — expect KPIs to
  # read ~3.7x worse, and the per-pair warning says so. PAIR_BASE_MODE is what
  # this pair ACTUALLY got: a rebuild that fails falls back to the archive
  # and is recorded as such.
  PAIR_BASE_MODE="archive"
  # The rebuilt base's stdout, when THIS run rebuilt one: its failed-call
  # WARNING refuses the pair's card like the scored leg's stats do.
  REBASE_STDOUT_ARGS=()
  if [[ "$BASE_MODE" != "archive" ]]; then
    INPUT_FROM="$INPUTS/claude-code-$FROM/binary-decompiled/src/entrypoints/index.js"
    if [[ -f "$INPUT_FROM" ]]; then
      REBASE="$WORK/$MODEL/${FROM}-rebased"
      REBASE_SEED_ARGS=()
      if [[ "$BASE_MODE" == "seeded" ]]; then
        REBASE_SEED_ARGS=(--prior-version "$PRIOR")
        echo "=== $PAIR: rebasing prior (re-humanify $FROM, current pipeline, SEEDED by the archive) ==="
      else
        echo "=== $PAIR: rebasing prior (re-humanify $FROM from SCRATCH, current pipeline, no prior) ==="
      fi
      rm -rf "$REBASE"
      mkdir -p "$WORK/$MODEL"
      REBASE_TOML="$WORK/$MODEL/${FROM}-rebased.toml"
      pipeline_config "$REBASE_TOML" ${CACHE_SETS[@]+"${CACHE_SETS[@]}"} \
        "diagnostics.log_file=$RESULTS/${FROM}-rebase.log" || exit 2
      "${PIPE_CMD[@]}" "$INPUT_FROM" --config "$REBASE_TOML" \
        --endpoint "$ENDPOINT" --model "$MODELNAME" -o "$REBASE" \
        ${REBASE_SEED_ARGS[@]+"${REBASE_SEED_ARGS[@]}"} -vv \
        > "$RESULTS/${FROM}-rebase.stdout" 2>&1
      if [[ -f "$REBASE/.humanify/humanified.js" ]]; then
        PRIOR="$REBASE/.humanify/humanified.js"
        PRIOR_LEDGER="$REBASE/.humanify/split-ledger.json"
        PAIR_BASE_MODE="$BASE_MODE"
        REBASE_STDOUT_ARGS=(--rebase-stdout "$RESULTS/${FROM}-rebase.stdout")
        echo "  prior rebased ($BASE_MODE) -> $PRIOR"
      else
        echo "  rebase FAILED; falling back to archive prior (recorded as baseMode archive)"
      fi
    else
      echo "  no input for $FROM; scoring against the archive prior (recorded as baseMode archive)"
    fi
  fi

  echo "=== [$((i + 1))/$npairs] $PAIR: pipeline ==="
  rm -rf "$OUT"
  # Run via experiments/lib/run-pipeline.ts rather than invoking tsx directly,
  # so ONE step records everything a later reader needs: exit code, wall time,
  # peak RSS across the whole process tree, cache entries written, active kill
  # switches, which PRIOR was used, commit + dirty flag, and artifact sizes.
  # Each of those was previously written by a different step or by none at all,
  # and the missing ones are exactly the facts whose absence produced wrong
  # published numbers (rule 10's cache, the ~3.7x archive-prior trap, a
  # heap ceiling nobody had ever measured).
  # The pipeline used to create this directory itself on first write; now the
  # run config lands here BEFORE the pipeline starts, so make it first.
  mkdir -p "$WORK/$MODEL"
  RUN_CFG="$WORK/$MODEL/$TO.runcfg.json"
  RUN_TOML="$WORK/$MODEL/$TO.toml"
  pipeline_config "$RUN_TOML" ${CACHE_SETS[@]+"${CACHE_SETS[@]}"} \
    "diagnostics.stats_json=$STATS" "diagnostics.log_file=$LOG" \
    "diagnostics.diagnostics_file=$WORK/$MODEL/$TO.diag.json" || exit 2
  ARGS_JSON=$(printf '%s\n' "$INPUT" --config "$RUN_TOML" \
    --endpoint "$ENDPOINT" --model "$MODELNAME" -o "$OUT" \
    --prior-version "$PRIOR" -vv | jq -R . | jq -s .)
  jq -n \
    --arg pair "$PAIR" --arg version "$TO" --arg runLabel "$MODEL" \
    --arg resultsDir "$RESULTS" --arg input "$INPUT" --arg prior "$PRIOR" \
    --arg baseMode "$PAIR_BASE_MODE" \
    --arg outputDir "$OUT" --arg repo "$REPO" \
    --arg stdoutPath "$RESULTS/$TO.stdout" --arg endpoint "$ENDPOINT" \
    --arg model "$MODELNAME" --arg effort "$EFFORT" \
    --argjson concurrency "$CONC" \
    --arg cacheDir "$LLM_CACHE_DIR" \
    --argjson args "$ARGS_JSON" --argjson extra "$RUNCFG_EXTRA" \
    --rawfile configToml "$RUN_TOML" \
    --argjson artifacts "$(printf '%s\n' "$OUT/.humanify/humanified.js" \
      "$OUT/.humanify/split-ledger.json" "$STATS" | jq -R . | jq -s .)" \
    '{"pair":$pair,"version":$version,"label":$runLabel,"resultsDir":$resultsDir,
      "input":$input,"prior":$prior,"baseMode":$baseMode,
      "outputDir":$outputDir,"repo":$repo,
      "args":$args,"configToml":$configToml,"stdoutPath":$stdoutPath,"endpoint":$endpoint,
      "model":$model,"reasoningEffort":$effort,"concurrency":$concurrency,
      "artifacts":$artifacts}
     + (if $cacheDir == "" then {} else {"cacheDir":$cacheDir} end)
     + $extra' \
    > "$RUN_CFG"
  npx tsx "$REPO/experiments/lib/run-pipeline.ts" "$RUN_CFG"
  PIPELINE_RC=$?
  # Record HOW the pipeline exited, not just whether it left files behind.
  # The artifact check below was hardened against a partial write, but a run
  # that writes EVERY artifact, prints "this run is marked failed" and exits 1
  # passed it unchanged — which is the state of 2.1.86 and 2.1.198 today, and
  # every KPI ever published for those pairs came from a tree the pipeline had
  # rejected. Scoring continues (dropping half the corpus loses more than it
  # protects) but the failure is now stamped where the summary must show it.
  #
  # It also REFUSES the card (exit 3) when the scored leg or the rebuilt base
  # lost LLM answers: the binary exits 0 on failed calls (fail-soft), each is
  # a hole in the names, and rule 11 says the KPIs cannot tell a lost name
  # from noise (concurrency audit 2026-10, R2 + recommendation 1c). A refused
  # pair gets no card — a stale one from an earlier run is removed — and the
  # summary banners it; the refusal prints the command that re-runs it.
  npx tsx "$REPO/experiments/lib/invariants.ts" "$RESULTS" "$TO" "$PIPELINE_RC" \
    --stats "$STATS" --pair "$PAIR" \
    ${REBASE_STDOUT_ARGS[@]+"${REBASE_STDOUT_ARGS[@]}"}
  STATUS_RC=$?
  if [[ $STATUS_RC -eq 3 ]]; then
    rm -f "$RESULTS/$TO.json"
    continue
  elif [[ $STATUS_RC -ne 0 ]]; then
    echo "  (warning: could not record run status for $PAIR)"
  fi
  if [[ $PIPELINE_RC -ne 0 ]]; then
    echo "PIPELINE EXITED $PIPELINE_RC for $PAIR — the run declared ITSELF invalid:"
    grep -E "^ERROR:" "$RESULTS/$TO.stdout" | head -5 | sed 's/^/    /'
    echo "  Scoring anyway, but summary.json is stamped and the table will say so."
  fi
  # Every artifact the run PROMISED, not just the first one. A crash between
  # writing the bundle and writing --stats-json used to leave a pair looking
  # successful: the boot gate passed (the tree really was written) and the
  # failure only surfaced as an ENOENT inside analyze.ts, three steps later.
  MISSING=""
  for artifact in \
    "$OUT/.humanify/humanified.js" \
    "$OUT/.humanify/split-ledger.json" \
    "$STATS"; do
    [[ -f "$artifact" ]] || MISSING="$MISSING $artifact"
  done
  if [[ -n "$MISSING" ]]; then
    echo "PIPELINE FAILED for $PAIR — missing:$MISSING"
    echo "  (see $RESULTS/$TO.stdout; an OOM here means the box ran out of memory — run the eval alone)"
    continue
  fi

  echo "=== $PAIR: churn analysis ==="
  # The statement-level churn above is position-BLIND: a byte-identical
  # statement emitted somewhere else costs nothing there and everything in
  # review (how Lever B v1's 118->119 regression hid). Passing both split trees
  # adds the `layout` block — real/naming/alias/REORDER in git lines. Costs a
  # few minutes per pair; --no-layout skips it.
  PRIOR_SRC="$(dirname "$(dirname "$PRIOR")")/src"
  LAYOUT_ARGS=()
  if [[ "$RUN_LAYOUT" == "1" && -d "$OUT/src" && -d "$PRIOR_SRC" ]]; then
    LAYOUT_ARGS=("$OUT/src" "$PRIOR_SRC")
    # vendor/ is its OWN scored surface, never folded into the src numbers
    # above (exp046). It was unscored until then, so every reference committed
    # before it prints `-` for the vendor columns -- which is not 0. Vendor
    # scoring rides on LAYOUT_ARGS because analyze.ts takes the trees
    # positionally; --no-vendor-score skips it.
    PRIOR_VENDOR="$(dirname "$(dirname "$PRIOR")")/vendor"
    if [[ "$RUN_VENDOR" == "1" && -d "$OUT/vendor" && -d "$PRIOR_VENDOR" ]]; then
      LAYOUT_ARGS+=("$OUT/vendor" "$PRIOR_VENDOR")
    fi
  fi
  NODE_OPTIONS="--max-old-space-size=$ANALYSIS_HEAP_MB" npx tsx "$HERE/analyze.ts" \
    "$OUT/.humanify/humanified.js" "$PRIOR" \
    "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
    "$STATS" "$PAIR" ${LAYOUT_ARGS[@]+"${LAYOUT_ARGS[@]}"} > "$RESULTS/$TO.json" \
    || echo "ANALYZE FAILED for $PAIR"

  # Human-readable evidence page (identifier + diff ledgers, funnel):
  # small HTML committed with the results; the big diag JSON stays in WORK.
  # A MISSING trail is said out loud, not folded into "REPORT PAGE FAILED",
  # so a reader can tell "no trail" from "the page failed".
  DIAG="$WORK/$MODEL/$TO.diag.json"
  if [[ ! -f "$DIAG" ]]; then
    echo "NO DIAGNOSTICS TRAIL for $PAIR at $DIAG — report page NOT written."
    echo "  See $RESULTS/$TO.stdout. Every KPI above is unaffected."
  else
    NODE_OPTIONS="--max-old-space-size=$ANALYSIS_HEAP_MB" npx tsx "$HERE/trail-report.ts" \
      "$DIAG" "$RESULTS/$TO-report.html" \
      "$OUT/.humanify/humanified.js" "$PRIOR" \
      "$OUT/.humanify/split-ledger.json" "$PRIOR_LEDGER" \
      > /dev/null 2>&1 || echo "REPORT PAGE FAILED for $PAIR"
  fi

  # Boot gate: an output that does not RUN is invalid no matter what the
  # noise KPIs say. `--version` must echo the version; the live `-p`
  # round-trip (--no-boot-prompt skips) exercises the loader
  # end-to-end. Loud on failure, never aborts the sweep; the verdict
  # lands in <TO>-boot.json next to the pair's stats.
  # bun is guaranteed present: lib/boot-gate.sh is fatal without it. Before
  # that, this branch fell through to "BOOT GATE SKIPPED" — CLAUDE.md still
  # warns about it — so a whole sweep could report green having never booted
  # a tree.
  if [[ -f "$OUT/run.cjs" ]]; then
    BOOT_VERSION=$(boot_version "$OUT")
    BOOT_PROMPT="skipped"
    if [[ "$RUN_BOOT_PROMPT" == "1" ]]; then
      # --model: BOOT_GATE_MODEL, pinned in lib/boot-gate.sh (the API rejects
      # the default model on every version this project walks since
      # 2026-09-18); recorded in the verdict below and in pipeline.json.
      BOOT_PROMPT=$(boot_prompt "$OUT")
    fi
    BOOT_OK=false
    if [[ "$BOOT_VERSION" == *"$TO"* ]]; then
      if [[ "$RUN_BOOT_PROMPT" != "1" || "$BOOT_PROMPT" == *"boot-ok"* ]]; then
        BOOT_OK=true
      fi
    fi
    printf '{"boot":{"version":"%s","prompt":"%s","model":"%s","ok":%s}}\n' \
      "$BOOT_VERSION" "$BOOT_PROMPT" "$BOOT_GATE_MODEL" "$BOOT_OK" > "$RESULTS/$TO-boot.json"
    if [[ "$BOOT_OK" == "true" ]]; then
      echo "BOOT GATE: OK ($BOOT_VERSION)"
    else
      echo "BOOT GATE FAILED for $PAIR: version='$BOOT_VERSION' prompt='$BOOT_PROMPT'"
    fi
  else
    echo "BOOT GATE FAILED for $PAIR: no run.cjs to boot"
  fi
done

# Self-hop (--no-self-hop skips): re-humanify the last attempted pair's TO
# version using its own fresh output as --prior-version. Same code on both
# sides means every statement is a hash-twin and every function exact-matches.
#
# TWO HALVES since 2026-09-25 (the 5b self-hop gate, 00-control §3), because
# "self-hop = 0" was never true COLD: every cold self-hop on record differs
# (96 / 92 / 114 / 180 ln — LLM re-rolls on the residue that reaches the
# model; self-hop-reference.json).
#   COLD  — the leg as it always ran. Its bundle diff count is judged against
#           the reference range by the summary (invariants.ts coldRangeVerdict).
#   WARM  — the same leg again, replaying
#           a scratch COPY of the cache the cold leg filled. With the model
#           held fixed it must reproduce the cold leg's tree byte for byte and
#           write 0 cache entries: determinism, rule 10's permitted use.
# Violations are logged loudly but never abort the sweep.
TO="${LAST_TO:-}"
INPUT="${LAST_INPUT:-}"

# Run one self-hop leg: <out> <stdout> <cache dir>. Returns its exit code.
self_hop_leg() {
  local out="$1" log="$2" cache="$3"
  pipeline_config "$out.toml" "llm.cache_dir=$cache" || return 2
  "${PIPE_CMD[@]}" "$INPUT" --config "$out.toml" \
    --endpoint "$ENDPOINT" --model "$MODELNAME" -o "$out" \
    --prior-version "$SELF_BASE/.humanify/humanified.js" \
    > "$log" 2>&1
}

count_files() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' '; }

# The warm leg: sets WARM_JSON. Compares the WHOLE tree against the cold
# leg's, minus the diagnostics-only sidecars neutrality.sh also excludes.
warm_self_hop() {
  local cache="$WORK/$MODEL/${TO}-selfhop-warm-cache"
  local out="$WORK/$MODEL/${TO}-selfhop-warm"
  echo "=== warm self-hop: replaying a copy of the cold leg's cache ==="
  rm -rf "$cache" "$out"
  cp -r "$SELF_CACHE" "$cache"
  local before after rc files lines identical ok
  before=$(count_files "$cache")
  self_hop_leg "$out" "$RESULTS/$TO-selfhop-warm.stdout" "$cache"
  rc=$?
  after=$(count_files "$cache")
  if [[ ! -f "$out/.humanify/humanified.js" ]]; then
    WARM_JSON='{"ran":false,"identical":false,"diffFiles":-1,"diffLines":-1,"cacheWrites":-1,"coldExit":'"$SELF_RC"',"warmExit":'"$rc"',"ok":false}'
    echo "!! WARM SELF-HOP DID NOT RUN for $TO (see $RESULTS/$TO-selfhop-warm.stdout)"
    return
  fi
  local ex=(--exclude=placement-stats.json --exclude=stage-hashes.json --exclude=library-report.json)
  files=$(diff -rq "${ex[@]}" "$SELF_OUT" "$out" 2>/dev/null | wc -l | tr -d ' ')
  lines=$(diff -rN "${ex[@]}" "$SELF_OUT" "$out" 2>/dev/null | grep -cE '^[<>]')
  identical=false; [[ "$files" == "0" ]] && identical=true
  ok=false
  [[ "$identical" == "true" && $((after - before)) -eq 0 && "$rc" == "$SELF_RC" ]] && ok=true
  WARM_JSON=$(jq -cn --argjson identical "$identical" --argjson files "$files" \
    --argjson lines "$lines" --argjson writes "$((after - before))" \
    --argjson coldExit "$SELF_RC" --argjson warmExit "$rc" --argjson ok "$ok" \
    '{ran:true, identical:$identical, diffFiles:$files, diffLines:$lines,
      cacheWrites:$writes, coldExit:$coldExit, warmExit:$warmExit, ok:$ok}')
  if [[ "$ok" == "true" ]]; then
    echo "WARM SELF-HOP: OK — byte-identical tree, 0 cache writes, exit $rc (cold leg filled $COLD_WRITES entries)"
  else
    echo "!! WARM SELF-HOP FAILED for $TO: $files file(s) / $lines ln differ, $((after - before)) cache write(s), exit $SELF_RC vs $rc"
    echo "   (must be 0 / 0 / 0 / equal — nondeterminism with the model held fixed)"
  fi
}

if [[ "$RUN_SELF_HOP" == "1" && -n "$TO" && -f "$WORK/$MODEL/$TO/.humanify/humanified.js" ]]; then
  SELF_BASE="$WORK/$MODEL/$TO"
  SELF_OUT="$WORK/$MODEL/${TO}-selfhop"
  echo "=== self-hop invariant: $TO vs its own output ==="
  rm -rf "$SELF_OUT"
  # What cache the COLD leg runs with (recorded; the summary refuses to judge
  # a leg that was not cold). The warm leg needs the answers the cold leg got,
  # so the cold leg records into a FRESH dir — still cold: it starts empty,
  # every prompt is live.
  COLD_CACHE="fresh"
  SELF_CACHE="$WORK/$MODEL/${TO}-selfhop-cache"
  rm -rf "$SELF_CACHE"
  if [[ -n "$LLM_CACHE_DIR" ]]; then
    cp -r "$LLM_CACHE_DIR" "$SELF_CACHE"
    COLD_CACHE="seeded"
    echo "  cold self-hop leg SEEDED from --llm-cache: it is NOT cold; its count will not be judged"
  else
    mkdir -p "$SELF_CACHE"
  fi
  COLD_BEFORE=$(count_files "$SELF_CACHE")
  self_hop_leg "$SELF_OUT" "$RESULTS/$TO-selfhop.stdout" "$SELF_CACHE"
  SELF_RC=$?
  COLD_WRITES=$(( $(count_files "$SELF_CACHE") - COLD_BEFORE ))
  # DID IT RUN, before asking whether it MATCHED. `cmp -s` is non-zero for a
  # MISSING file just as it is for a different one, and `diff` against a
  # missing file prints its error to stderr and counts zero lines — so a
  # self-hop that produced nothing at all used to report
  # "VIOLATED: 0 diff lines", a violation with a count of zero. Observed live
  # when the self-hop pipeline OOM'd.
  SELF_MISSING=""
  for artifact in \
    "$SELF_OUT/.humanify/humanified.js" \
    "$SELF_OUT/.humanify/split-ledger.json"; do
    [[ -f "$artifact" ]] || SELF_MISSING="$SELF_MISSING $artifact"
  done
  if [[ -n "$SELF_MISSING" ]]; then
    printf '{"selfHop":{"version":"%s","ran":false,"identical":null,"diffLines":null}}\n' \
      "$TO" > "$RESULTS/$TO-self-hop.json"
    echo "SELF-HOP DID NOT RUN for $TO — missing:$SELF_MISSING"
    echo "  This is NOT a passing invariant and NOT a violation; the check did not happen."
    echo "  (see $RESULTS/$TO-selfhop.stdout; an OOM here means the box ran out of memory — run the eval alone)"
  else
  SELF_OK=true
  SELF_DIFF=0
  if ! cmp -s "$SELF_BASE/.humanify/humanified.js" "$SELF_OUT/.humanify/humanified.js"; then
    SELF_OK=false
    SELF_DIFF=$(diff "$SELF_BASE/.humanify/humanified.js" "$SELF_OUT/.humanify/humanified.js" | grep -cE '^[<>]')
  fi
  if ! cmp -s "$SELF_BASE/.humanify/split-ledger.json" "$SELF_OUT/.humanify/split-ledger.json"; then
    SELF_OK=false
  fi
  if [[ "$SELF_OK" == "true" ]]; then
    echo "SELF-HOP INVARIANT: OK — byte-identical bundle and ledger"
  else
    echo "COLD SELF-HOP: $SELF_DIFF bundle diff lines (cache: $COLD_CACHE; see $RESULTS/$TO-selfhop.stdout)"
    echo "  reference range on record: $(jq -r '[.coldDiffLines[]] | "\(min)-\(max) ln over \(length) cold runs"' "$HERE/self-hop-reference.json") on $(jq -r .version "$HERE/self-hop-reference.json") — judged in the summary"
  fi
  warm_self_hop
  # Per version, not a fixed filename: with several pairs the old path meant
  # only the last one survived.
  jq -cn --arg v "$TO" --argjson identical "$SELF_OK" --argjson diff "$SELF_DIFF" \
    --arg coldCache "$COLD_CACHE" --argjson warm "$WARM_JSON" \
    '{selfHop:{version:$v, ran:true, identical:$identical, diffLines:$diff,
      coldCache:$coldCache, warm:$warm}}' \
    > "$RESULTS/$TO-self-hop.json"
  fi
fi

echo "=== summarizing model '$MODEL' ==="
npx tsx "$HERE/summarize.ts" "$MODEL"
