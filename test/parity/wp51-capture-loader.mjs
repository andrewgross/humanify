// WP5.1 capture: the module-load half. Registered by wp51-capture-hook.mjs.
// Rewrites the (tsx-transformed) stable-split.ts so `stableSplitFromCode`
// is wrapped: the placement trail is armed around the ORIGINAL call (and
// the caller's own trail state restored after, with this call's rows
// appended when the caller had it armed — exactly what an armed trail
// would have recorded), the namer callbacks are wrapped to log each
// (requests → proposals) pair, and the inputs, the trail and the result
// are handed to `globalThis.__wp51rec`. Observation only: the options'
// values are passed through, and the result is returned untouched.
export async function load(url, context, nextLoad) {
  const result = await nextLoad(url, context);
  if (!url.endsWith("/src/split/stable-split.ts") || result.source == null) {
    return result;
  }
  let source =
    typeof result.source === "string"
      ? result.source
      : Buffer.from(result.source).toString("utf8");
  const decl = "async function stableSplitFromCode(";
  if (source.split(decl).length !== 2) {
    throw new Error(`wp51 loader: expected one "${decl}" in ${url}`);
  }
  source = source.replace(
    decl,
    "async function __wp51_orig_stableSplitFromCode("
  );
  source += `
async function stableSplitFromCode(code, options = {}) {
  const rec = globalThis.__wp51rec;
  if (!rec) return __wp51_orig_stableSplitFromCode(code, options);
  const trail = placementTrail;
  const saved = { enabled: trail.enabled, tiers: trail.tiers, trails: trail.trails };
  trail.reset(true);
  const namerLog = [];
  const wrap = (fn, kind) =>
    fn &&
    (async (arg) => {
      const out = await fn(arg);
      namerLog.push({ kind, arg, out });
      return out;
    });
  const opts = {
    ...options,
    namer: wrap(options.namer, "namer"),
    reviser: wrap(options.reviser, "reviser"),
    mintNamer: wrap(options.mintNamer, "mintNamer")
  };
  let out;
  let error;
  try {
    out = await __wp51_orig_stableSplitFromCode(code, opts);
  } catch (e) {
    error = e;
  }
  const report = trail.report();
  trail.enabled = saved.enabled;
  trail.tiers = saved.tiers;
  trail.trails = saved.trails;
  if (saved.enabled) {
    saved.trails.push(...report.trails);
    for (const [k, v] of Object.entries(report.tiers)) {
      saved.tiers[k] = (saved.tiers[k] ?? 0) + v;
    }
  }
  rec({ code, options, namerLog, result: out, trail: report, error: error?.message });
  if (error) throw error;
  return out;
}
`;
  return { ...result, source };
}
