import type { Perturbation } from "../types.js";
import { identity } from "./identity.js";
import { addConsoleLogTo } from "./add-console-log.js";
import { renameProperty } from "./rename-property.js";
import { swapPropertyOrder } from "./swap-property-order.js";

/**
 * A named perturbation applied to a specific corpus item.
 * The target function name depends on the corpus — we pair them explicitly
 * so the runner knows which (corpus, perturbation) combinations make sense.
 */
export interface PerturbationPlan {
  corpusId: string;
  perturbation: Perturbation;
}

export function buildDefaultPlans(): PerturbationPlan[] {
  return [
    // Sanity check — every corpus item gets the identity perturbation
    { corpusId: "r1b-synthetic", perturbation: identity },
    { corpusId: "disambiguation", perturbation: identity },

    // r1b-synthetic: perturb each function individually
    {
      corpusId: "r1b-synthetic",
      perturbation: addConsoleLogTo("notify")
    },
    {
      corpusId: "r1b-synthetic",
      perturbation: addConsoleLogTo("reset")
    },
    {
      corpusId: "r1b-synthetic",
      perturbation: addConsoleLogTo("display")
    },

    // disambiguation: perturb the already-targeted function + others
    {
      corpusId: "disambiguation",
      perturbation: addConsoleLogTo("updateFromInput")
    },
    {
      corpusId: "disambiguation",
      perturbation: addConsoleLogTo("processAll")
    },

    // Property key perturbations — test memberKey feature
    {
      corpusId: "r1b-synthetic",
      perturbation: renameProperty("getCount", "fetchCount")
    },
    {
      corpusId: "r1b-synthetic",
      perturbation: renameProperty("setLabel", "applyLabel")
    },

    // Property order swap — should be 100% on all minifiers
    { corpusId: "r1b-synthetic", perturbation: swapPropertyOrder },
    { corpusId: "disambiguation", perturbation: swapPropertyOrder }
  ];
}
