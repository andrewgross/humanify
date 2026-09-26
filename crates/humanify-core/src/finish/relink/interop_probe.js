// The shim's __toESM / __toCommonJS against Bun's own (2.1.86's prologue,
// names restored). Usage: node interop_probe.js <shim.js>; prints "same".
var __create = Object.create,
  __getProtoOf = Object.getPrototypeOf,
  __defProp = Object.defineProperty,
  __getOwnPropNames = Object.getOwnPropertyNames,
  __getOwnPropDesc = Object.getOwnPropertyDescriptor,
  __hasOwnProp = Object.prototype.hasOwnProperty;
function __accessProp(H) {
  return this[H];
}
var c1,
  c2,
  cm,
  bunToESM = (H, _, q) => {
    var $ = H != null && typeof H === "object";
    if ($) {
      var K = _ ? (c1 ??= new WeakMap()) : (c2 ??= new WeakMap()),
        O = K.get(H);
      if (O) return O;
    }
    q = H != null ? __create(__getProtoOf(H)) : {};
    let T =
      _ || !H || !H.__esModule
        ? __defProp(q, "default", { value: H, enumerable: !0 })
        : q;
    for (let z of __getOwnPropNames(H))
      if (!__hasOwnProp.call(T, z))
        __defProp(T, z, { get: __accessProp.bind(H, z), enumerable: !0 });
    if ($) K.set(H, T);
    return T;
  },
  bunToCommonJS = (H) => {
    var _ = (cm ??= new WeakMap()).get(H),
      q;
    if (_) return _;
    if (
      ((_ = __defProp({}, "__esModule", { value: !0 })),
      (H && typeof H === "object") || typeof H === "function")
    ) {
      for (var $ of __getOwnPropNames(H))
        if (!__hasOwnProp.call(_, $))
          __defProp(_, $, {
            get: __accessProp.bind(H, $),
            enumerable: !(q = __getOwnPropDesc(H, $)) || q.enumerable
          });
    }
    return cm.set(H, _), _;
  };

const shim = require(process.argv[2]);
const show = (o) =>
  JSON.stringify([
    Object.keys(o),
    Object.entries(o).map(([k, v]) => [
      k,
      typeof v === "object" ? JSON.stringify(v) : String(v)
    ]),
    o.__esModule,
    Object.getPrototypeOf(o) === Object.prototype,
    Object.getOwnPropertyNames(o)
  ]);
const attempt = (f) => {
  try {
    return show(f());
  } catch (e) {
    return "throws " + e.constructor.name;
  }
};
class Base {
  hello() {
    return 1;
  }
}
const cases = [
  () => ({ a: 1 }),
  () => ({ __esModule: true, default: 2, b: 3 }),
  () => function f() {},
  () => new Base(),
  () => null,
  () => undefined
];
const out = [];
for (const [esm, cjs] of [
  [bunToESM, bunToCommonJS],
  [shim.__toESM, shim.__toCommonJS]
]) {
  const row = [];
  for (const c of cases)
    for (const mode of [undefined, 1]) row.push(attempt(() => esm(c(), mode)));
  row.push(typeof esm(new Base()).hello);
  const m = { a: 1 };
  row.push(
    esm(m) === esm(m),
    esm(m, 1) === esm(m, 1),
    esm(m, 1) === esm(m),
    cjs(m) === cjs(m)
  );
  const live = { v: 1 };
  const view = esm(live);
  live.v = 2;
  row.push(view.v);
  const ns = {};
  Object.defineProperty(ns, "x", { get: () => 5, enumerable: true });
  Object.defineProperty(ns, "hidden", { value: 6, enumerable: false });
  row.push(show(cjs(ns)), show(cjs(() => 1)), show(cjs({ a: 1 })));
  out.push(JSON.stringify(row));
}
if (out[0] !== out[1]) {
  console.error(out.join("\n"));
  process.exit(1);
}
console.log("same");
