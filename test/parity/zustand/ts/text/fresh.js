const createStoreImpl = t => {
  let e;
  const c = new Set();
  const o = (t, o) => {
    const r = typeof t == "function" ? t(e) : t;
    if (!Object.is(r, e)) {
      const t = e;
      if (o ?? (typeof r != "object" || r === null)) {
        e = r;
      } else {
        e = Object.assign({}, e, r);
      }
      c.forEach(c => c(e, t));
    }
  };
  const r = () => e;
  const n = {
    setState: o,
    getState: r,
    getInitialState: () => a,
    subscribe: t => (c.add(t), () => c.delete(t))
  };
  const a = e = t(o, r, n);
  return n;
};
export const createStore = t => t ? createStoreImpl(t) : createStoreImpl;