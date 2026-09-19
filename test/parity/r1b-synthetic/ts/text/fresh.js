export function createStore(n) {
  let t = n;
  let e = [];
  function o() {
    return t;
  }
  function u() {
    return t;
  }
  function i(n) {
    t = n;
  }
  function r(n) {
    t = n;
  }
  return {
    getCount: o,
    getLabel: u,
    setCount: i,
    setLabel: r,
    processAll: function (n) {
      for (let t = 0; t < n.length; t++) {
        if (n[t] > 0) {
          console.log(o());
        }
      }
    },
    display: function () {
      return String(u());
    },
    updateFromInput: function (n) {
      if (typeof n == "number") {
        i(n);
      } else if (typeof n == "string") {
        i(parseInt(n));
      }
    },
    initialize: function (n) {
      r(n.label);
    },
    subscribe: function (n) {
      e.push(n);
    },
    unsubscribe: function (n) {
      let t = e.indexOf(n);
      if (t >= 0) {
        e.splice(t, 1);
      }
    },
    notify: function () {
      for (let n = 0; n < e.length; n++) {
        e[n](t);
      }
    },
    reset: function () {
      let e = t;
      t = n;
      return e;
    }
  };
}