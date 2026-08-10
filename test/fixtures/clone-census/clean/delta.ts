/**
 * Fixture for test/clone-census.test.ts (clean case). No function here has a
 * structural twin in gamma.ts — the test asserts the strict census exits 0.
 * Never imported by production code.
 */
export interface HistogramBucket {
  upperBound: number;
  count: number;
}

export function bucketize(
  samples: readonly number[],
  bounds: readonly number[]
): HistogramBucket[] {
  const buckets = bounds.map((upperBound) => ({ upperBound, count: 0 }));
  for (const sample of samples) {
    const bucket = buckets.find((b) => sample <= b.upperBound);
    if (bucket) {
      bucket.count += 1;
    }
  }
  return buckets;
}
