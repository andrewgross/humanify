// WP5.2 Math.log probe: V8's Math.log (fdlibm) bit patterns for the
// cluster-assign IDF inputs n / (1 + d) over the statement counts a real
// bundle has, plus a seeded sweep of arbitrary doubles. Rust's f64::ln is
// the platform libm and may differ by an ulp; the port uses fdlibm.
// Output: test/parity/wp51-math-log.json ([[inputBitsHex, outputBitsHex]]).
//
//   npx tsx test/parity/wp51-math-log-probe.ts > test/parity/wp51-math-log.json
const bits = (x: number): string => {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, x);
  return view.getBigUint64(0).toString(16).padStart(16, "0");
};
const inputs: number[] = [
  0,
  -0,
  -1,
  1,
  2,
  0.5,
  Number.MIN_VALUE,
  2.2e-308,
  Number.MAX_VALUE,
  Infinity,
  NaN
];
for (const n of [3, 7, 64, 100, 1000, 19966, 23442, 31839, 35903]) {
  for (let d = 0; d <= Math.min(n, 400); d++) inputs.push(n / (1 + d));
}
let seed = 0x106;
const rnd = (): number => {
  seed = (seed * 1103515245 + 12345) & 0x7fffffff;
  return seed / 0x7fffffff;
};
for (let k = 0; k < 3000; k++)
  inputs.push(rnd() * 10 ** Math.floor(rnd() * 12 - 4));
process.stdout.write(
  `${JSON.stringify(inputs.map((x) => [bits(x), bits(Math.log(x))]))}\n`
);
