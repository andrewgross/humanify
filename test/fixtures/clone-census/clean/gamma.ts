/**
 * Fixture for test/clone-census.test.ts (clean case). No function here has a
 * structural twin in delta.ts — the test asserts the strict census exits 0.
 * Never imported by production code.
 */
export interface WindowSpec {
  size: number;
  step: number;
}

export function slidingWindows(
  values: readonly number[],
  spec: WindowSpec
): number[][] {
  const windows: number[][] = [];
  for (let start = 0; start + spec.size <= values.length; start += spec.step) {
    windows.push(values.slice(start, start + spec.size));
  }
  if (windows.length === 0 && values.length > 0) {
    windows.push([...values]);
  }
  return windows;
}
