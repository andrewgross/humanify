// WP3.2 synthetic (prior, fresh) pairs for the transfer-stage probe
// (wp32-transfer-probe.ts). Each case aims at one tier; the probe records
// whatever the REAL TS decides, and the Rust replays it.
export interface TransferCase {
  name: string;
  prior: string;
  fresh: string;
}

export const CASES: TransferCase[] = [
  {
    name: "exact-match slots, an external vote, the binding cascade",
    prior: `var config = { port: 8080 };
function readPort(settings) { return settings.port + config.port; }
function startServer(options) { return readPort(options); }
startServer({});
`,
    fresh: `var c = { port: 8080 };
function r(s) { return s.port + c.port; }
function a(o) { return r(o); }
a({});
`
  },
  {
    name: "a swapped pair: rejected both ways, the retry cycle-break lands both",
    prior: `function alpha(first) { return first + 1; }
function beta(second) { return second * 22; }
alpha(beta(3));
`,
    fresh: `function beta(first) { return first + 1; }
function alpha(second) { return second * 22; }
beta(alpha(3));
`
  },
  {
    name: "a module binding named by two agreeing exact votes",
    prior: `var settings = loadAlpha();
function readX() { return settings.x; }
function readY() { return settings.y + 1; }
readX(readY());
`,
    fresh: `var q = loadBeta();
function f1() { return q.x; }
function f2() { return q.y + 1; }
f1(f2());
`
  },
  {
    name: "a single exact vote pinned by role",
    prior: `var cache = new Map([["a", 1], ["b", 2], ["c", 3], ["e", 4]]);
function lookup(key) { return cache.get(key); }
lookup("a");
`,
    fresh: `var m = new Map([["a", 1], ["b", 2], ["d", 3], ["e", 4]]);
function g(k) { return m.get(k); }
g("a");
`
  },
  {
    name: "a drifted helper named by its exact-matched callers",
    prior: `function helperFn(value) { return value * 2 + 7; }
function useOne() { return helperFn(1); }
function useTwo() { return helperFn(2) + 1; }
useOne(useTwo());
`,
    fresh: `function h(value) { if (value) { return value - 7; } return value * 3; }
function v1() { return h(1); }
function v2() { return h(2) + 1; }
v1(v2());
`
  },
  {
    name: "a close match: signature pairs and an aligned body local",
    prior: `function computeTotal(items, rate) {
  const subtotal = items.length;
  const taxed = subtotal * rate;
  return taxed + 1;
}
computeTotal([], 2);
`,
    fresh: `function c(i, r) {
  const s = i.length;
  const t = s * r;
  console.log(t);
  return t + 1;
}
c([], 2);
`
  },
  {
    name: "a closure capture from an exact-matched inner function",
    prior: `function outerWork(list) {
  let total = 0;
  const addOne = (item) => { total += item.size; };
  list.forEach(addOne);
  console.log("done");
  return total;
}
outerWork([]);
`,
    fresh: `function o(l) {
  let t = 0;
  const a = (i) => { t += i.size; };
  l.forEach(a);
  return t;
}
o([]);
`
  }
];
