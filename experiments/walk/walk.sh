#!/usr/bin/env bash
#
# A real VERSION WALK of one pipeline, the way releases would actually be
# processed: the first version cold with no prior, then every hop humanifies
# version N with version N-1's OWN output as --prior-version.
#
#   experiments/walk/walk.sh --pipeline rust|ts --versions <from>..<to> --out <dir>
#       [--bin <path>]            rust: binary to walk (default: this repo's
#                                 target/release/humanify, built first)
#       [--ts-tree <dir>]         ts: the frozen TS pipeline checkout
#                                 (default /work/tsctl-fec64e5-walk: a copy of
#                                 /work/tsctl-fec64e5 src/ + its OWN npm ci —
#                                 that tree symlinks the repo node_modules,
#                                 which lost `commander` at the cutover)
#       [--resume]                continue a walk in <dir>; finished hops skipped
#       [--endpoint <url>]        LLM endpoint override (default pairs.json)
#       [--inputs-base <dir>]     override pairs.json inputsBase
#       [--slots <wrapper>]       run each hop's pipeline through this slot
#                                 wrapper (default /work/heavy-run.sh: one of
#                                 three machine-wide slots — HEAVY-RUN-RULE)
#       [--lock <file>]           instead: serialize on a single flock file
#       [--no-lock]               run with neither (fixture-scale only)
#       [--no-boot-prompt]        boot gate checks --version only
#       [--force-mixed]           accept a --bin built from a dirty tree
#       [--reboot]                with --resume: re-run the boot gate on finished
#                                 hops whose boot FAILED (re-recorded, marked)
#       [--no-score]              do not score each finished hop in the
#                                 background (report.ts scores it later)
#
# <from>..<to> selects every version in inputsBase between the two (inclusive,
# semver order) that has an input bundle; a comma list selects exactly those.
# The walk is only as contiguous as inputsBase: a version missing there is
# skipped silently by the range (the manifest lists what actually ran).
#
# Layout of <dir>:
#   manifest.json           pipeline, binary sha256 / TS commit, versions,
#                           endpoint/model — written once, checked on --resume
#   bin/humanify            (rust) a FROZEN COPY of the binary; every hop runs
#                           this copy, so a rebuild mid-walk cannot mix commits
#   trees/<v>/              the hop's output tree (src/, vendor/, .humanify/…)
#   runs/<v>.hop.json       THE per-hop record: exit, wall, peak RSS, boot, prior
#   runs/<v>-run.json       run-pipeline.ts manifest (wall, peak RSS, cache +0)
#   runs/<v>.stats.json     the pipeline's --stats-json
#   runs/<v>.stdout|.log    pipeline stdout / -vv log
#   runs/<v>-boot.json      boot gate, BOTH halves recorded separately
#   cards/<a>__<b>.json     the eval scorecard of hop a->b (report.ts card)
#
# Rules this obeys (CLAUDE.md): cold — no --llm-cache, ever (rule 10); the
# endpoint/model come from 034's pairs.json so a walk and the eval ask the
# same model; the boot gate pins BOOT_GATE_MODEL (lib/boot-gate.sh); each
# pipeline run holds one /work/heavy-run.sh slot, so a Rust and a TS walk
# launched together run concurrently without joining an OOM pile-up. Only the
# pipeline holds a slot — the boot gate and background scoring do not.
#
# ROBUST OVERNIGHT: a boot failure is recorded, never fatal. A hop that exits
# non-zero but wrote its tree is recorded and the walk continues on that tree
# (the pipeline exits 1 on a rename-invariant failure having written
# everything). The walk aborts only when a hop leaves NO tree, because the next
# hop then has no prior.
set -uo pipefail

# FREEZE THIS SCRIPT. bash reads a script as it runs, so editing walk.sh in the
# repo mid-walk (a merge, a checkout) would change a running overnight walk
# under it. Re-exec from a private copy; HERE/REPO still name the checkout.
if [[ -z "${WALK_SH_FROZEN:-}" ]]; then
  _frozen="$(mktemp "${TMPDIR:-/tmp}/walk.sh.XXXXXX")"
  cp "$0" "$_frozen"
  WALK_SH_FROZEN="$(cd "$(dirname "$0")" && pwd)" exec bash "$_frozen" "$@"
fi
HERE="$WALK_SH_FROZEN"
REPO="$(cd "$HERE/../.." && pwd)"
CFG="$REPO/experiments/034-eval-harness/pairs.json"
export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"

PIPELINE=""
VERSIONS_ARG=""
OUT=""
BIN="$REPO/target/release/humanify"
TS_TREE="/work/tsctl-fec64e5-walk"
RESUME=0
ENDPOINT_OVERRIDE=""
INPUTS_OVERRIDE=""
LOCK=""
SLOTS="/work/heavy-run.sh"
BOOT_PROMPT_ON=1
FORCE_MIXED=0
SCORE=1
REBOOT=0
SCORE_PIDS=()
HEAP_MB=65536
while [[ $# -gt 0 ]]; do
  case "$1" in
    --pipeline)       PIPELINE="$2"; shift ;;
    --versions)       VERSIONS_ARG="$2"; shift ;;
    --out)            OUT="$2"; shift ;;
    --bin)            BIN="$2"; shift ;;
    --ts-tree)        TS_TREE="$2"; shift ;;
    --resume)         RESUME=1 ;;
    --endpoint)       ENDPOINT_OVERRIDE="$2"; shift ;;
    --inputs-base)    INPUTS_OVERRIDE="$2"; shift ;;
    --slots)          SLOTS="$2"; LOCK=""; shift ;;
    --lock)           LOCK="$2"; SLOTS=""; shift ;;
    --no-lock)        LOCK=""; SLOTS="" ;;
    --no-boot-prompt) BOOT_PROMPT_ON=0 ;;
    --force-mixed)    FORCE_MIXED=1 ;;
    --no-score)       SCORE=0 ;;
    --reboot)         REBOOT=1 ;;
    *) echo "walk.sh: unknown arg $1 (see header)" >&2; exit 2 ;;
  esac
  shift
done
[[ "$PIPELINE" == "rust" || "$PIPELINE" == "ts" ]] \
  || { echo "walk.sh: --pipeline rust|ts required" >&2; exit 2; }
[[ -n "$VERSIONS_ARG" && -n "$OUT" ]] \
  || { echo "walk.sh: --versions and --out required" >&2; exit 2; }
command -v jq >/dev/null || { echo "walk.sh: jq required" >&2; exit 2; }

# Fatal without bun: a walk that cannot boot its trees is not gated.
# shellcheck source=../lib/boot-gate.sh
source "$REPO/experiments/lib/boot-gate.sh"

INPUTS="${INPUTS_OVERRIDE:-$(jq -r .inputsBase "$CFG")}"
ENDPOINT="${ENDPOINT_OVERRIDE:-$(jq -r .llm.endpoint "$CFG")}"
MODELNAME=$(jq -r .llm.model "$CFG")
APIKEY=$(jq -r .llm.apiKey "$CFG")
EFFORT=$(jq -r .llm.reasoningEffort "$CFG")
CONC=$(jq -r .llm.concurrency "$CFG")

input_of() { echo "$INPUTS/claude-code-$1/binary-decompiled/src/entrypoints/index.js"; }
# semver a <= b
ver_le() { [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" == "$1" ]]; }

VERSIONS=()
if [[ "$VERSIONS_ARG" == *..* ]]; then
  FROM="${VERSIONS_ARG%%..*}"
  TO="${VERSIONS_ARG##*..}"
  while IFS= read -r v; do
    if ver_le "$FROM" "$v" && ver_le "$v" "$TO" && [[ -f "$(input_of "$v")" ]]; then
      VERSIONS+=("$v")
    fi
  done < <(find "$INPUTS" -maxdepth 1 -type d -name 'claude-code-*' \
             | sed 's|.*/claude-code-||' | sort -V)
else
  IFS=',' read -r -a VERSIONS <<< "$VERSIONS_ARG"
  for v in "${VERSIONS[@]}"; do
    [[ -f "$(input_of "$v")" ]] || { echo "walk.sh: no input for $v: $(input_of "$v")" >&2; exit 2; }
  done
fi
[[ ${#VERSIONS[@]} -ge 1 ]] || { echo "walk.sh: no versions selected by '$VERSIONS_ARG' in $INPUTS" >&2; exit 2; }

mkdir -p "$OUT/trees" "$OUT/runs" "$OUT/bin"
OUT="$(cd "$OUT" && pwd)"
MANIFEST="$OUT/manifest.json"

if [[ -f "$MANIFEST" && "$RESUME" != "1" ]]; then
  echo "walk.sh: $OUT already holds a walk; pass --resume to continue it" >&2
  exit 2
fi

# ── WHAT RUNS: frozen for the whole walk ────────────────────────────────
if [[ "$PIPELINE" == "rust" ]]; then
  FROZEN="$OUT/bin/humanify"
  if [[ "$RESUME" == "1" && -x "$FROZEN" ]]; then
    echo "walk: resuming with the frozen binary $FROZEN"
  else
    ROOT_COMMIT=""
    BIN_ROOT="$(cd "$(dirname "$BIN")/../.." 2>/dev/null && pwd || true)"
    [[ -n "$BIN_ROOT" ]] && ROOT_COMMIT=$(git -C "$BIN_ROOT" rev-parse HEAD 2>/dev/null || true)
    FORCE_ARGS=()
    [[ "$FORCE_MIXED" == "1" ]] && FORCE_ARGS=(--force-mixed)
    BIN_JSON=$(cd "$REPO" && npx tsx experiments/lib/pipeline-bin.ts "$BIN" "$ROOT_COMMIT" \
      ${FORCE_ARGS[@]+"${FORCE_ARGS[@]}"})
    rc=$?
    [[ $rc -eq 0 && -n "$BIN_JSON" ]] || { echo "walk.sh: could not build/record $BIN (exit $rc)" >&2; exit 1; }
    cp "$BIN" "$FROZEN"
    echo "$BIN_JSON" > "$OUT/bin/record.json"
  fi
  BIN_JSON=$(cat "$OUT/bin/record.json")
  SHA=$(sha256sum "$FROZEN" | cut -d' ' -f1)
  [[ "$SHA" == "$(jq -r .sha256 <<< "$BIN_JSON")" ]] \
    || { echo "walk.sh: frozen binary sha256 does not match its record" >&2; exit 1; }
  PIPE_CMD_JSON=$(jq -cn --arg b "$FROZEN" '[$b]')
  RUN_REPO="$REPO"
  PIPE_DESC=$(jq -c '{kind:"rust-bin", bin:., frozenCopy:"'"$FROZEN"'"}' <<< "$BIN_JSON")
else
  [[ -f "$TS_TREE/src/index.ts" ]] || { echo "walk.sh: no TS pipeline at $TS_TREE/src/index.ts" >&2; exit 2; }
  # A git checkout records its HEAD; a plain copy (the default — see header)
  # carries the commit it was copied from in COMMIT.
  TS_COMMIT=$(git -C "$TS_TREE" rev-parse HEAD 2>/dev/null || cat "$TS_TREE/COMMIT" 2>/dev/null || echo "")
  TS_DIRTY=$( [[ -n "$(git -C "$TS_TREE" status --porcelain --untracked-files=no 2>/dev/null)" ]] && echo true || echo false )
  SRC_SHA=$(cd "$TS_TREE" && find src -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | cut -d' ' -f1)
  PIPE_CMD_JSON=$(jq -cn --arg t "$TS_TREE/src/index.ts" '["npx","tsx",$t]')
  RUN_REPO="$TS_TREE"
  PIPE_DESC=$(jq -cn --arg tree "$TS_TREE" --arg c "$TS_COMMIT" --argjson d "$TS_DIRTY" \
    --arg srcSha "$SRC_SHA" --argjson cmd "$PIPE_CMD_JSON" \
    '{kind:"ts", tree:$tree, commit:$c, dirty:$d, srcSha256:$srcSha, command:$cmd}')
fi

# PREFLIGHT: the pipeline must at least start. A TS tree whose node_modules
# lost a dependency (the cutover pruned the repo's) dies in 0 s on every hop.
PREFLIGHT_CMD=($(jq -r '.[]' <<< "$PIPE_CMD_JSON"))
if ! (cd "$RUN_REPO" && timeout 300 "${PREFLIGHT_CMD[@]}" --help > "$OUT/preflight.log" 2>&1); then
  echo "walk.sh: PREFLIGHT FAILED — '${PREFLIGHT_CMD[*]} --help' does not run (see $OUT/preflight.log)" >&2
  exit 2
fi

VERSIONS_JSON=$(printf '%s\n' "${VERSIONS[@]}" | jq -R . | jq -sc .)
NEW_MANIFEST=$(jq -n --arg pipeline "$PIPELINE" --argjson desc "$PIPE_DESC" \
  --argjson versions "$VERSIONS_JSON" --arg range "$VERSIONS_ARG" \
  --arg inputs "$INPUTS" --arg endpoint "$ENDPOINT" --arg model "$MODELNAME" \
  --arg effort "$EFFORT" --argjson conc "$CONC" --arg lock "${SLOTS:+slots:$SLOTS}${LOCK:+flock:$LOCK}" \
  --arg bootModel "$BOOT_GATE_MODEL" --argjson bootPrompt "$BOOT_PROMPT_ON" \
  --arg harness "$(git -C "$REPO" rev-parse HEAD 2>/dev/null)" \
  '{pipeline:$pipeline, pipelineRecord:$desc, range:$range, versions:$versions,
    inputsBase:$inputs, llm:{endpoint:$endpoint, model:$model,
    reasoningEffort:$effort, concurrency:$conc, cache:"OFF (cold, rule 10)"},
    heavyLock:$lock, bootGate:{model:$bootModel, prompt:($bootPrompt==1)},
    harnessCommit:$harness}')
if [[ -f "$MANIFEST" ]]; then
  # --resume: the walk must continue with the SAME pipeline and model.
  for k in .pipeline .pipelineRecord .llm.endpoint .llm.model .inputsBase; do
    a=$(jq -c "$k" "$MANIFEST"); b=$(jq -c "$k" <<< "$NEW_MANIFEST")
    if [[ "$a" != "$b" ]]; then
      echo "walk.sh: --resume refused: $k differs ($a vs $b)" >&2
      exit 2
    fi
  done
  # A resumed walk may be EXTENDED (a later --versions end); keep the union.
  jq --argjson v "$VERSIONS_JSON" '.versions = ((.versions + $v) | unique_by(.)) ' \
    "$MANIFEST" > "$MANIFEST.tmp" && mv "$MANIFEST.tmp" "$MANIFEST"
else
  jq --arg t "$(date -u +%FT%TZ)" '. + {startedAt:$t}' <<< "$NEW_MANIFEST" > "$MANIFEST"
fi

echo "walk: $PIPELINE over ${#VERSIONS[@]} versions: ${VERSIONS[*]}"
echo "walk: out $OUT; endpoint $ENDPOINT ($MODELNAME); LLM cache OFF; heavy-run '${SLOTS:+slots $SLOTS}${LOCK:+flock $LOCK}'"

LOCK_CMD=()
if [[ -n "$SLOTS" ]]; then
  [[ -x "$SLOTS" ]] || { echo "walk.sh: slot wrapper $SLOTS is not executable (--lock <file> or --no-lock to choose otherwise)" >&2; exit 2; }
  LOCK_CMD=("$SLOTS")
elif [[ -n "$LOCK" ]]; then
  LOCK_CMD=(flock "$LOCK")
fi

# The booted tree IS Claude Code: launched from inside a Claude Code session
# (an agent running this walk) it inherits CLAUDECODE and refuses to start
# ("cannot be launched inside another Claude Code session") — which reads as a
# boot FAILURE of a tree that is fine. Strip the session's variables.
BOOT_ENV=(env -u CLAUDECODE -u CLAUDE_CODE_ENTRYPOINT -u CLAUDE_CODE_CHILD_SESSION
  -u CLAUDE_CODE_SESSION_ID -u CLAUDE_CODE_MESSAGING_SOCKET
  -u CLAUDE_CODE_MESSAGING_TOKEN -u CLAUDE_CODE_BRIDGE_SESSION_ID)

# Boot both halves and RECORD them; never exits.
boot_record() {
  local dir="$1" v="$2" dest="$3"
  local version="" prompt="skipped"
  if [[ -f "$dir/run.cjs" ]]; then
    version=$( (cd "$dir" && timeout 60 "${BOOT_ENV[@]}" bun run.cjs --version 2>&1 | tail -1) || true )
    if [[ "$BOOT_PROMPT_ON" == "1" ]]; then
      prompt=$( (cd "$dir" && timeout 120 "${BOOT_ENV[@]}" bun run.cjs -p "say exactly: boot-ok" --model "$BOOT_GATE_MODEL" 2>&1 | tail -1) || true )
    fi
  else
    version="(no run.cjs)"
  fi
  local vok=false pok=false
  [[ "$version" == *"$v"* ]] && vok=true
  [[ "$BOOT_PROMPT_ON" != "1" || "$prompt" == *"boot-ok"* ]] && pok=true
  jq -n --arg version "$version" --arg prompt "$prompt" --argjson vok "$vok" \
    --argjson pok "$pok" --arg model "$BOOT_GATE_MODEL" \
    '{version:$version, prompt:$prompt, versionOk:$vok, promptOk:$pok,
      ok:($vok and $pok), model:$model}' > "$dest"
  jq -r '"  boot: " + (if .ok then "OK" else "FAIL" end) + "  --version=\(.versionOk) -p=\(.promptOk)"' "$dest"
}

PRIOR_V=""
for V in "${VERSIONS[@]}"; do
  TREE="$OUT/trees/$V"
  HOP="$OUT/runs/$V.hop.json"
  if [[ -f "$HOP" && -f "$TREE/.humanify/humanified.js" ]]; then
    if [[ "$REBOOT" == "1" && "$(jq -r .boot.ok "$HOP")" != "true" ]]; then
      echo "=== $V: re-running the boot gate (--reboot)"
      boot_record "$TREE" "$V" "$OUT/runs/$V-boot.json"
      jq --slurpfile boot "$OUT/runs/$V-boot.json" \
        '.boot = ($boot[0] + {rerecorded:(now | todate)})' "$HOP" > "$HOP.tmp" \
        && mv "$HOP.tmp" "$HOP"
    fi
    echo "=== $V: done ($(jq -r '"exit \(.exitCode), \(.wallSeconds)s, boot \(.boot.ok)"' "$HOP")) — skipped"
    PRIOR_V="$V"
    continue
  fi
  PRIOR_ARGS=()
  PRIOR_PATH=""
  if [[ -n "$PRIOR_V" ]]; then
    PRIOR_PATH="$OUT/trees/$PRIOR_V/.humanify/humanified.js"
    PRIOR_ARGS=(--prior-version "$PRIOR_PATH")
  fi
  echo "=== $(date -u +%H:%M:%S) hop $V (prior: ${PRIOR_V:-COLD}) ==="
  rm -rf "$TREE"
  STATS="$OUT/runs/$V.stats.json"
  rm -f "$STATS"
  ARGS_JSON=$(printf '%s\n' "$(input_of "$V")" --split \
    --endpoint "$ENDPOINT" --model "$MODELNAME" --api-key "$APIKEY" \
    --reasoning-effort "$EFFORT" -c "$CONC" -o "$TREE" \
    ${PRIOR_ARGS[@]+"${PRIOR_ARGS[@]}"} --stats-json "$STATS" \
    -vv --log-file "$OUT/runs/$V.log" | jq -R . | jq -s .)
  RUN_CFG="$OUT/runs/$V.runcfg.json"
  jq -n --arg pair "${PRIOR_V:-COLD}->$V" --arg version "$V" \
    --arg runLabel "walk-$PIPELINE" --arg resultsDir "$OUT/runs" \
    --arg input "$(input_of "$V")" --arg prior "$PRIOR_PATH" \
    --arg outputDir "$TREE" --arg repo "$RUN_REPO" \
    --arg stdoutPath "$OUT/runs/$V.stdout" --arg endpoint "$ENDPOINT" \
    --arg model "$MODELNAME" --arg effort "$EFFORT" --argjson conc "$CONC" \
    --argjson heapMb "$HEAP_MB" --argjson args "$ARGS_JSON" \
    --argjson command "$PIPE_CMD_JSON" \
    --argjson artifacts "$(printf '%s\n' "$TREE/.humanify/humanified.js" \
      "$TREE/.humanify/split-ledger.json" "$STATS" | jq -R . | jq -s .)" \
    '{pair:$pair, version:$version, label:$runLabel, resultsDir:$resultsDir,
      input:$input, prior:$prior, outputDir:$outputDir, repo:$repo, args:$args,
      command:$command, stdoutPath:$stdoutPath, endpoint:$endpoint,
      model:$model, reasoningEffort:$effort, concurrency:$conc,
      heapMb:$heapMb, artifacts:$artifacts}' > "$RUN_CFG" \
    || { echo "walk.sh: could not write $RUN_CFG — aborting" >&2; exit 1; }
  if [[ "$PIPELINE" == "rust" ]]; then
    { jq --argjson bin "$BIN_JSON" '. + {bin:$bin}' "$RUN_CFG" > "$RUN_CFG.tmp" && mv "$RUN_CFG.tmp" "$RUN_CFG"; } \
      || { echo "walk.sh: could not write $RUN_CFG — aborting" >&2; exit 1; }
  fi
  rm -f "$OUT/runs/$V-run.json"

  T_LOCK=$(date +%s)
  # run-pipeline.ts: exit code, wall (excluding the lock wait), peak RSS of the
  # whole process tree (GNU time is not installed here), cache writes.
  ${LOCK_CMD[@]+"${LOCK_CMD[@]}"} npx tsx "$REPO/experiments/lib/run-pipeline.ts" "$RUN_CFG"
  RC=$?
  T_END=$(date +%s)

  TREE_OK=false
  [[ -f "$TREE/.humanify/humanified.js" && -f "$TREE/.humanify/split-ledger.json" ]] && TREE_OK=true
  if [[ "$TREE_OK" != "true" ]]; then
    echo "walk: hop $V exited $RC and wrote NO tree — aborting (the next hop has no prior)" >&2
    grep -E "^ERROR:" "$OUT/runs/$V.stdout" 2>/dev/null | head -5 | sed 's/^/    /' >&2
    exit 1
  fi
  [[ $RC -ne 0 ]] && echo "  !! exit $RC but the tree was written — recorded, walk continues on it"

  boot_record "$TREE" "$V" "$OUT/runs/$V-boot.json"

  RUNJ="$OUT/runs/$V-run.json"
  # run-pipeline.ts always writes it; if it crashed, record the hop anyway.
  [[ -f "$RUNJ" ]] || echo '{"wallSeconds":null,"outcome":{"errors":["run-pipeline.ts wrote no manifest"]},"config":{"cache":{}}}' > "$RUNJ"
  jq -n --arg v "$V" --arg prior "$PRIOR_V" --argjson rc "$RC" \
    --argjson elapsed "$((T_END - T_LOCK))" \
    --slurpfile run "$RUNJ" --slurpfile boot "$OUT/runs/$V-boot.json" \
    '{version:$v, priorVersion:(if $prior == "" then null else $prior end),
      cold:($prior == ""), exitCode:$rc, wallSeconds:$run[0].wallSeconds,
      lockWaitSeconds:(if $run[0].wallSeconds == null then null else $elapsed - $run[0].wallSeconds end),
      peakRssMb:$run[0].outcome.peakRssMb,
      cacheWritten:$run[0].config.cache.written,
      errors:$run[0].outcome.errors, boot:$boot[0],
      finishedAt:(now | todate)}' > "$HOP.tmp" && mv "$HOP.tmp" "$HOP" \
    || { echo "walk.sh: could not record $HOP — aborting" >&2; exit 1; }
  # Score the finished hop with the eval's analyzer while the next hop runs
  # (report.ts caches the card; a failure here costs nothing — report.ts
  # re-scores whatever is missing). Outside the heavy lock: it is scoring, not
  # a pipeline run.
  if [[ -n "$PRIOR_V" && "$SCORE" == "1" ]]; then
    npx tsx "$HERE/report.ts" card "$OUT" "$PRIOR_V" "$V" > "$OUT/runs/$V.card.log" 2>&1 &
    SCORE_PIDS+=($!)
  fi
  PRIOR_V="$V"
done

if [[ ${#SCORE_PIDS[@]} -gt 0 ]]; then
  echo "walk: waiting for ${#SCORE_PIDS[@]} background scorecard(s)"
  wait "${SCORE_PIDS[@]}"
fi
echo "SENTINEL: WALK-DONE $PIPELINE $OUT"
