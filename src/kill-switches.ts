/**
 * Every pass switch, as data, with ONE predicate — set upfront from flags.
 *
 * ## History (two generations of the same lesson)
 *
 * Generation 1: these were env vars (`HUMANIFY_NO_*`) read inline at 14
 * sites in three mutually incompatible ways (`=== "1"` / `!== "1"` /
 * truthy), so `FLAG=0` disabled five of them and not the other nine. This
 * registry unified the predicate.
 *
 * Generation 2 (2026-08-12, owner direction): the env vars themselves are
 * gone. Ambient environment reads are the same defect one layer up — config
 * readable from anywhere, invisible to `--help`, unvalidated, absent from
 * the run's recorded flags. Switches are now typed as CLI flags
 * (`--disable a,b` / `--probe c`), validated against this registry at
 * arg-parse (an unknown name is fatal and lists the valid ones), applied
 * once via `configureKillSwitches`, and read through `switchOn`.
 *
 * The registry is still the single source of truth: a switch that is not
 * here is not a switch, and the switch census fails the gate on any
 * `switchOn("name")` literal that is not registered.
 *
 * The generated runner (`runnable-scaffold.ts`) reads `HUMANIFY_STRIP_USING`
 * and `__HUMANIFY_USING_REEXEC` from env, but that code executes in the
 * EMITTED tree and cannot import from here — deliberately out of scope.
 */

/** What a switch does, and what established it. */
interface KillSwitch {
  /** Prose for `--help` and for anyone asking "what can I A/B?". */
  what: string;
  /** The experiment that introduced or gated it — where the numbers live. */
  since: string;
  /**
   * `disable` switches turn a shipped pass OFF (`--disable`); `probe`
   * switches turn instrumentation ON (`--probe`). The kinds are separate
   * flags so "list of things I turned off" stays a true statement.
   */
  kind: "disable" | "probe";
}

/**
 * The registry. A switch that is not here is not a switch.
 *
 * Ordered by pipeline stage, because that is how someone bisecting a
 * regression reads it: naming, then placement, then emission, then vendor,
 * then post-tree.
 */
export const KILL_SWITCHES = {
  "family-permute": {
    what: "naming: the family-permute pass that rotates same-family names into their prior slots",
    since: "exp048",
    kind: "disable"
  },
  "shingle-probe": {
    what: "naming: a per-close-pair shingle census (instrumentation)",
    since: "exp053",
    kind: "probe"
  },
  "content-anchor": {
    what: "placement: the content-anchor tier (rare string literals identify a prior statement)",
    since: "exp041",
    kind: "disable"
  },
  "anchor-preempt": {
    what: "placement: promoting the content anchor above the name vote when every declared name is a minted counter",
    since: "exp042",
    kind: "disable"
  },
  "anchor-nearident": {
    what: "placement: the near-identical disjunct of the anchor preempt (a twin differing by a few lines out of hundreds)",
    since: "exp043",
    kind: "disable"
  },
  "allsame-vote": {
    what: "placement: the all-same tier, which rescues a statement whose declared names disagree but whose single-home voters are unanimous",
    since: "exp041",
    kind: "disable"
  },
  "empty-decl-hash-guard": {
    what: "placement: the refusal to let the fingerprint claim a declaration with no initializers (its mask is only a declarator count)",
    since: "exp058",
    kind: "disable"
  },
  "registrar-exemption": {
    what: "placement: exempting export registrars from the load-order barrier",
    since: "exp049",
    kind: "disable"
  },
  "emit-align": {
    what: "emission: aligning within-file statement order to the prior release",
    since: "exp037/038",
    kind: "disable"
  },
  "name-align": {
    what: "emission: keying the emission aligner on (hash, declared name) instead of hash alone",
    since: "exp050",
    kind: "disable"
  },
  "vendor-inherit": {
    what: "vendor: reusing a prior release's bytes for a library whose structural signature is unchanged",
    since: "exp046",
    kind: "disable"
  },
  "manifest-prior-order": {
    what: "vendor: emitting _bun-modules.json in the prior release's order",
    since: "exp047",
    kind: "disable"
  },
  "post-split-reconcile": {
    what: "post-tree: the per-file rename reconcile against the prior tree",
    since: "exp054",
    kind: "disable"
  },
  "fossil-graded-content": {
    what: "placement: matching an enclosure by GRADED shape similarity (tree n-grams + literals) when per-statement equality has collapsed",
    since: "exp078",
    kind: "disable"
  },
  "fossil-graph-position": {
    what: "placement: matching an enclosure to its prior self by IMPORT-GRAPH POSITION when content overlap is too low for any content tier",
    since: "exp078",
    kind: "disable"
  },
  "fossil-split": {
    what: "placement: assigning statements by the bundle's own module fossils (bun __esm segments) with module-keyed file naming",
    since: "exp070",
    kind: "disable"
  }
} as const satisfies Record<string, KillSwitch>;

export type KillSwitchName = keyof typeof KILL_SWITCHES;

/** Names of a given kind, for validation messages and --help. */
export function switchNames(kind: "disable" | "probe"): KillSwitchName[] {
  return (Object.keys(KILL_SWITCHES) as KillSwitchName[]).filter(
    (n) => KILL_SWITCHES[n].kind === kind
  );
}

const active = new Set<KillSwitchName>();

/**
 * Apply the parsed `--disable` / `--probe` lists. Called ONCE from the CLI
 * action (and from tests, after `resetKillSwitchesForTests`). An unknown or
 * wrong-kind name throws with the valid list — a switch that could not take
 * effect must never look accepted (the exact failure the env generation
 * allowed: an exported typo was silently nothing).
 */
export function configureKillSwitches(opts: {
  disable?: string[];
  probe?: string[];
}): void {
  const apply = (names: string[] | undefined, kind: "disable" | "probe") => {
    for (const raw of names ?? []) {
      const name = raw.trim();
      if (name === "") continue;
      const entry = (KILL_SWITCHES as Record<string, KillSwitch>)[name];
      if (!entry || entry.kind !== kind) {
        const flag = kind === "disable" ? "--disable" : "--probe";
        throw new Error(
          `${flag}: unknown ${kind} switch "${name}" — valid: ${switchNames(kind).join(", ")}`
        );
      }
      active.add(name as KillSwitchName);
    }
  };
  apply(opts.disable, "disable");
  apply(opts.probe, "probe");
}

/**
 * Tests only: clear applied switches between cases.
 *
 * @internal Consumed by the switch contract tests and every pass test that
 * ablates via configureKillSwitches — knip:prod exempt via this tag.
 */
export function resetKillSwitchesForTests(): void {
  active.clear();
}

/**
 * Whether a switch was applied. Typed to the registry, so a typo is a
 * compile error rather than a switch that silently never fires — which is
 * the failure mode this whole file is about. For `disable` switches, true
 * means the pass is OFF; for `probe` switches, true means the probe is ON.
 */
export function switchOn(name: KillSwitchName): boolean {
  return active.has(name);
}

/**
 * Every applied switch, for the run log / selection record — so a
 * non-default run says so in its own recorded configuration.
 *
 * @internal Consumed by run manifests (experiments/lib/run-pipeline.ts) and
 * unit tests — knip:prod exempt via `tags` in package.json.
 */
export function activeKillSwitches(): KillSwitchName[] {
  return [...active].sort();
}
