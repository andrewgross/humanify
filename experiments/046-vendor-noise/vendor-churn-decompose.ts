/**
 * Task A report — print the full vendor churn decomposition for one hop.
 *
 *   npx tsx experiments/046-vendor-noise/vendor-churn-decompose.ts \
 *     <priorVendorDir> <freshVendorDir> [label]
 *
 * The scoring itself lives in `vendor-churn.ts`, which `analyze.ts` also uses,
 * so the offline report and the gate's KPI can never drift apart.
 */
import { decomposeVendorChurn } from "./vendor-churn.js";

const [priorDir, freshDir, label] = process.argv.slice(2);
if (!priorDir || !freshDir) {
  console.error(
    "usage: vendor-churn-decompose.ts <priorVendor> <freshVendor> [label]"
  );
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      label: label ?? "",
      priorDir,
      freshDir,
      ...decomposeVendorChurn(priorDir, freshDir)
    },
    null,
    2
  )
);
