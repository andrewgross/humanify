export default function mitt(e) {
  return {
    all: e = e || new Map(),
    on(t, n) {
      const s = e.get(t);
      if (s) {
        s.push(n);
      } else {
        e.set(t, [n]);
      }
    },
    off(t, n) {
      const s = e.get(t);
      if (s) {
        if (n) {
          s.splice(s.indexOf(n) >>> 0, 1);
        } else {
          e.set(t, []);
        }
      }
    },
    emit(t, n) {
      let s = e.get(t);
      if (s) {
        s.slice().map(e => {
          e(n);
        });
      }
      s = e.get("*");
      if (s) {
        s.slice().map(e => {
          e(t, n);
        });
      }
    }
  };
}