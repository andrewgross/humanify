// An ES module reached only through require() (from legacy.cjs). Its
// top-level loop is a side effect, so Bun wraps the module in a lazy
// `__esm` init (`init_late`) — the shape the fossil split reads.
export const LATE_PREFIX = "late#";

const steps = [];
for (let i = 0; i < 3; i++) {
  steps.push(i * 2);
}

export function late(n) {
  return LATE_PREFIX + String(n * 2) + "/" + steps.length;
}
