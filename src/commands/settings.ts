import { env } from "../env.js";
import { parseNumber } from "../number-utils.js";
import { DEFAULT_LLM_TIMEOUT_MS } from "./default-args.js";

/**
 * Every setting, resolved ONCE, here.
 *
 * ## Why
 *
 * Settings used to enter the program four ways — CLI options, `process.env`,
 * values re-derived downstream from `opts.*`, and files on disk — and the
 * re-derivation is where the bugs were:
 *
 *   - `DEFAULT_LLM_TIMEOUT_MS` lived in three places with two different values
 *   - `reasoningEffort` was parsed twice inside one function
 *   - `skipLibraries` defaulted to `true` independently in `unminify.ts` and
 *     `rename/plugin.ts`, neither of them the CLI
 *   - `parseNumber(opts.x)` was called at seven sites, several of them deep
 *     inside `runPipeline` rather than where `opts` was built
 *
 * None of those was a live bug when found, which is the point: each was one
 * edit away from becoming one, and nothing would have caught it.
 *
 * ## What is NOT here, deliberately
 *
 * **Kill switches.** `src/kill-switches.ts` owns those and reads them at CALL
 * time on purpose: the tests that prove each switch works set the variable and
 * then invoke the pipeline, so a value frozen at startup would make them assert
 * nothing.
 *
 * **`moduleConcurrency` when unset.** It stays `undefined` rather than being
 * resolved to a number, because its default is bundler-aware
 * (`defaultModuleConcurrency`) and the bundler is not detected yet. Resolving
 * it here would silently pin every bundle to the non-esbuild lane.
 */

/** CLI options this resolver reads. A subset of the commander-parsed object. */
export interface SettingsInput {
  endpoint?: string;
  model?: string;
  apiKey?: string;
  timeout?: string;
  retries?: string;
  concurrency?: string;
  batchSize?: string;
  maxRetries?: string;
  maxFreeRetries?: string;
  laneThreshold?: string;
  llmCache?: string;
  reasoningEffort?: string;
  skipLibraries?: boolean;
  namingFloor?: boolean;
  namingFloorSweep?: boolean;
  reconcilePriorDiff?: boolean;
  priorVersion?: string;
}

/** The shipped noise levers, all defaulting ON. */
export interface LeverSettings {
  namingFloor: boolean;
  /** Implied off when the floor is off — the sweep cannot run without it. */
  namingFloorSweep: boolean;
  /** Implied off with no prior version — there is nothing to reconcile against. */
  reconcilePriorDiff: boolean;
}

export interface Settings {
  /** Commander always supplies these (its option defaults carry the env
   * fallback, which is the CLI layer's job and what `--help` prints), so they
   * are non-optional here and the provider does not have to re-check. */
  readonly endpoint: string;
  readonly model: string;
  readonly apiKey: string;
  readonly timeout: number;
  readonly maxTokens: number | undefined;
  readonly reasoningEffort: ReasoningEffort | undefined;
  readonly llmCacheDir: string | undefined;
  readonly concurrency: number;
  /** `undefined` means "let the bundler-aware default decide" — see above. */
  readonly moduleConcurrency: number | undefined;
  readonly retryAttempts: number | undefined;
  readonly batchSize: number | undefined;
  readonly maxRetriesPerIdentifier: number | undefined;
  readonly maxFreeRetries: number | undefined;
  readonly laneThreshold: number | undefined;
  readonly skipLibraries: boolean;
  readonly levers: Readonly<LeverSettings>;
}

/** Raised rather than `process.exit` so a caller can report it in context. */
export class MissingApiKeyError extends Error {
  constructor() {
    super(
      "API key required. Provide --api-key, or set HUMANIFY_API_KEY or " +
        "OPENAI_API_KEY environment variable."
    );
    this.name = "MissingApiKeyError";
  }
}

/** Exactly what `LLMConfig.reasoningEffort` accepts — not widened here. */
type ReasoningEffort = "low" | "medium" | "high";
const EFFORTS: readonly ReasoningEffort[] = ["low", "medium", "high"];

/** Parse a reasoning-effort string, rejecting anything not in the enum. */
export function parseReasoningEffort(
  value: string | undefined
): ReasoningEffort | undefined {
  if (value === undefined) return undefined;
  if (!(EFFORTS as readonly string[]).includes(value)) {
    throw new Error(
      `invalid reasoning effort ${JSON.stringify(value)} — expected one of ${EFFORTS.join(", ")}`
    );
  }
  return value as ReasoningEffort;
}

/** A number from a CLI string, or undefined when absent. */
const num = (v: string | undefined): number | undefined =>
  v === undefined ? undefined : parseNumber(v);

export function resolveSettings(opts: SettingsInput): Settings {
  const apiKey =
    opts.apiKey ?? env("HUMANIFY_API_KEY") ?? env("OPENAI_API_KEY");
  if (!apiKey) throw new MissingApiKeyError();

  const namingFloor = opts.namingFloor ?? true;
  const moduleConcurrencyEnv = env("HUMANIFY_MODULE_CONCURRENCY");

  if (!opts.endpoint || !opts.model) {
    throw new Error(
      "endpoint and model must be supplied by the CLI layer (commander's " +
        "option defaults carry the env fallback); resolveSettings does not " +
        "re-derive them."
    );
  }

  return Object.freeze({
    endpoint: opts.endpoint,
    model: opts.model,
    apiKey,
    timeout: num(opts.timeout) ?? DEFAULT_LLM_TIMEOUT_MS,
    maxTokens: num(env("HUMANIFY_MAX_TOKENS")),
    reasoningEffort: parseReasoningEffort(
      opts.reasoningEffort ?? env("HUMANIFY_REASONING_EFFORT")
    ),
    llmCacheDir: opts.llmCache ?? env("HUMANIFY_LLM_CACHE"),
    concurrency: num(opts.concurrency) ?? 0,
    moduleConcurrency: num(moduleConcurrencyEnv),
    retryAttempts: num(opts.retries),
    batchSize: num(opts.batchSize),
    maxRetriesPerIdentifier: num(opts.maxRetries),
    maxFreeRetries: num(opts.maxFreeRetries),
    laneThreshold: num(opts.laneThreshold),
    skipLibraries: opts.skipLibraries ?? true,
    levers: Object.freeze({
      namingFloor,
      namingFloorSweep: (opts.namingFloorSweep ?? true) && namingFloor,
      reconcilePriorDiff:
        (opts.reconcilePriorDiff ?? true) && Boolean(opts.priorVersion)
    })
  });
}
