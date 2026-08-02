/**
 * Every environment kill switch, as data, with ONE predicate.
 *
 * ## Why this exists
 *
 * These accumulated one experiment at a time and were read inline at 14 sites
 * in three mutually incompatible ways:
 *
 *   `=== "1"`   NO_EMIT_ALIGN, NO_CONTENT_ANCHOR, NO_ANCHOR_PREEMPT, …
 *   `!== "1"`   NO_EMPTY_DECL_HASH_GUARD, NO_ANCHOR_NEARIDENT, NO_ALLSAME_VOTE
 *   truthy      NO_FAMILY_PERMUTE, NO_NAME_ALIGN, NO_VENDOR_INHERIT, …
 *
 * So `FLAG=0` disabled five of them and not the other nine — and the worst
 * pair sat in the SAME functions: `cjs-emit.ts` gated emission alignment on
 * `=== "1"` and name alignment on bare truthiness, two lines apart.
 *
 * Nothing could enumerate them either, which is why `054/pinned-ab.sh` had a
 * flag name hard-coded until exp058 needed a different one.
 *
 * ## The predicate
 *
 * **Set means the literal string `"1"`. Nothing else counts.** Audited before
 * changing: every occurrence in the repo — source comments, experiment scripts,
 * experiment write-ups, tests — already passes `=1`, so no caller changes
 * meaning. `HUMANIFY_AMBIGUITY_PROBE` is deliberately absent: it carries a
 * PATH, not a boolean, and truthiness is correct for it.
 *
 * The generated runner (`runnable-scaffold.ts`) also reads `HUMANIFY_STRIP_USING`
 * and `__HUMANIFY_USING_REEXEC`, but that code executes in the EMITTED tree and
 * cannot import from here. It already uses `=== "1"`.
 */

/** What a switch turns off, and what established it. */
interface KillSwitch {
  /** Prose for `--help` and for anyone asking "what can I A/B?". */
  what: string;
  /** The experiment that introduced or gated it — where the numbers live. */
  since: string;
}

/**
 * The registry. A switch that is not here is not a switch.
 *
 * Ordered by pipeline stage, because that is how someone bisecting a
 * regression reads it: naming, then placement, then emission, then vendor,
 * then post-tree.
 */
export const KILL_SWITCHES = {
  HUMANIFY_NO_FAMILY_PERMUTE: {
    what: "naming: the family-permute pass that rotates same-family names into their prior slots",
    since: "exp048"
  },
  HUMANIFY_SHINGLE_PROBE: {
    what: "naming: ENABLES a per-close-pair shingle census (instrumentation, not a kill switch)",
    since: "exp053"
  },
  HUMANIFY_NO_CONTENT_ANCHOR: {
    what: "placement: the content-anchor tier (rare string literals identify a prior statement)",
    since: "exp041"
  },
  HUMANIFY_NO_ANCHOR_PREEMPT: {
    what: "placement: promoting the content anchor above the name vote when every declared name is a minted counter",
    since: "exp042"
  },
  HUMANIFY_NO_ANCHOR_NEARIDENT: {
    what: "placement: the near-identical disjunct of the anchor preempt (a twin differing by a few lines out of hundreds)",
    since: "exp043"
  },
  HUMANIFY_NO_ALLSAME_VOTE: {
    what: "placement: the all-same tier, which rescues a statement whose declared names disagree but whose single-home voters are unanimous",
    since: "exp041"
  },
  HUMANIFY_NO_EMPTY_DECL_HASH_GUARD: {
    what: "placement: the refusal to let the fingerprint claim a declaration with no initializers (its mask is only a declarator count)",
    since: "exp058"
  },
  HUMANIFY_NO_REGISTRAR_EXEMPTION: {
    what: "placement: exempting export registrars from the load-order barrier",
    since: "exp049"
  },
  HUMANIFY_NO_EMIT_ALIGN: {
    what: "emission: aligning within-file statement order to the prior release",
    since: "exp037/038"
  },
  HUMANIFY_NO_NAME_ALIGN: {
    what: "emission: keying the emission aligner on (hash, declared name) instead of hash alone",
    since: "exp050"
  },
  HUMANIFY_NO_VENDOR_INHERIT: {
    what: "vendor: reusing a prior release's bytes for a library whose structural signature is unchanged",
    since: "exp046"
  },
  HUMANIFY_NO_MANIFEST_PRIOR_ORDER: {
    what: "vendor: emitting _bun-modules.json in the prior release's order",
    since: "exp047"
  },
  HUMANIFY_NO_POST_SPLIT_RECONCILE: {
    what: "post-tree: the per-file rename reconcile against the prior tree",
    since: "exp054"
  }
} as const satisfies Record<string, KillSwitch>;

export type KillSwitchName = keyof typeof KILL_SWITCHES;

/**
 * Whether an environment flag is set, by the one definition of "set".
 *
 * Typed to the registry, so a typo is a compile error rather than a switch
 * that silently never fires — which is the failure mode this whole file is
 * about.
 *
 * Reads `process.env` at call time on purpose: the tests that prove each
 * switch works set the variable and then invoke the pipeline, and a value
 * frozen at startup would make those tests assert nothing. When settings move
 * to an up-front resolver, that pattern is the constraint to design around.
 */
export function envFlag(name: KillSwitchName): boolean {
  return process.env[name] === "1";
}

/** Every switch currently set — for a run log, so a non-default run says so. */
export function activeKillSwitches(): KillSwitchName[] {
  return (Object.keys(KILL_SWITCHES) as KillSwitchName[]).filter(envFlag);
}
