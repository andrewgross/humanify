(() => {
  var Qt = Object.create;
  var p = Object.defineProperty;
  var Ut = Object.getOwnPropertyDescriptor;
  var Vt = Object.getOwnPropertyNames;
  var Wt = Object.getPrototypeOf;
  var Yt = Object.prototype.hasOwnProperty;
  var Zt = /* @__PURE__ */ ((t) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(t, {
    get: (e, r) => (typeof require !== "undefined" ? require : e)[r]
  }) : t)(function(t) {
    if (typeof require !== "undefined") return require.apply(this, arguments);
    throw Error('Dynamic require of "' + t + '" is not supported');
  });
  var T = (t, e, r) => () => {
    if (r) throw r[0];
    try {
      return t && (e = t(t = 0)), e;
    } catch (i) {
      throw r = [i], i;
    }
  };
  var n = (t, e) => () => {
    try {
      return e || t((e = { exports: {} }).exports, e), e.exports;
    } catch (r) {
      throw e = 0, r;
    }
  };
  var X = (t, e) => {
    for (var r in e)
      p(t, r, { get: e[r], enumerable: true });
  };
  var _ = (t, e, r, i) => {
    if (e && typeof e === "object" || typeof e === "function") {
      for (let u of Vt(e))
        if (!Yt.call(t, u) && u !== r)
          p(t, u, { get: () => e[u], enumerable: !(i = Ut(e, u)) || i.enumerable });
    }
    return t;
  };
  var o = (t, e, r) => (r = t != null ? Qt(Wt(t)) : {}, _(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    e || !t || !t.__esModule ? p(r, "default", { value: t, enumerable: true }) : r,
    t
  ));
  var te = (t) => _(p({}, "__esModule", { value: true }), t);

  // src/vendor/mod00.cjs
  var z = n((Fr, j) => {
    "use strict";
    var ee = 0;
    function re(t) {
      return t * 2 + Number(ee);
    }
    function ne() {
      return "mod00:";
    }
    j.exports = { twice: re, tag: ne };
  });

  // src/vendor/mod01.cjs
  var D = n((Lr, B) => {
    "use strict";
    var oe = 1;
    function ie(t) {
      return t * 2 + Number(oe);
    }
    function ue() {
      return "mod01:";
    }
    B.exports = { twice: ie, tag: ue };
  });

  // src/vendor/mod02.cjs
  var H = n((Rr, G) => {
    "use strict";
    var se = 2;
    function ae(t) {
      return t * 2 + Number(se);
    }
    function ce() {
      return "mod02:";
    }
    G.exports = { twice: ae, tag: ce };
  });

  // src/vendor/mod03.cjs
  var Q = n((Pr, K) => {
    "use strict";
    var me = 3;
    function pe(t) {
      return t * 2 + Number(me);
    }
    function fe() {
      return "mod03:";
    }
    K.exports = { twice: pe, tag: fe };
  });

  // src/vendor/mod04.cjs
  var V = n((qr, U) => {
    "use strict";
    var de = 4;
    function ge(t) {
      return t * 2 + Number(de);
    }
    function we() {
      return "mod04:";
    }
    U.exports = { twice: ge, tag: we };
  });

  // src/vendor/mod05.cjs
  var Y = n((Cr, W) => {
    "use strict";
    var be = 5;
    function $e(t) {
      return t * 2 + Number(be);
    }
    function ve() {
      return "mod05:";
    }
    W.exports = { twice: $e, tag: ve };
  });

  // src/vendor/mod06.cjs
  var tt = n((Ir, Z) => {
    "use strict";
    var xe = 6;
    function he(t) {
      return t * 2 + Number(xe);
    }
    function le() {
      return "mod06:";
    }
    Z.exports = { twice: he, tag: le };
  });

  // src/vendor/mod07.cjs
  var rt = n((Jr, et) => {
    "use strict";
    var Ne = 7;
    function ye(t) {
      return t * 2 + Number(Ne);
    }
    function Se() {
      return "mod07:";
    }
    et.exports = { twice: ye, tag: Se };
  });

  // src/vendor/mod08.cjs
  var ot = n((Or, nt) => {
    "use strict";
    var Me = 8;
    function ke(t) {
      return t * 2 + Number(Me);
    }
    function Ae() {
      return "mod08:";
    }
    nt.exports = { twice: ke, tag: Ae };
  });

  // src/vendor/mod09.cjs
  var ut = n((Tr, it) => {
    "use strict";
    var Ee = 9;
    function Fe(t) {
      return t * 2 + Number(Ee);
    }
    function Le() {
      return "mod09:";
    }
    it.exports = { twice: Fe, tag: Le };
  });

  // src/vendor/mod10.cjs
  var at = n((Xr, st) => {
    "use strict";
    var Re = 10;
    function Pe(t) {
      return t * 2 + Number(Re);
    }
    function qe() {
      return "mod10:";
    }
    st.exports = { twice: Pe, tag: qe };
  });

  // src/vendor/mod11.cjs
  var mt = n((_r, ct) => {
    "use strict";
    var Ce = 11;
    function Ie(t) {
      return t * 2 + Number(Ce);
    }
    function Je() {
      return "mod11:";
    }
    ct.exports = { twice: Ie, tag: Je };
  });

  // src/vendor/mod12.cjs
  var ft = n((jr, pt) => {
    "use strict";
    var Oe = 12;
    function Te(t) {
      return t * 2 + Number(Oe);
    }
    function Xe() {
      return "mod12:";
    }
    pt.exports = { twice: Te, tag: Xe };
  });

  // src/vendor/mod13.cjs
  var gt = n((zr, dt) => {
    "use strict";
    var _e = 13;
    function je(t) {
      return t * 2 + Number(_e);
    }
    function ze() {
      return "mod13:";
    }
    dt.exports = { twice: je, tag: ze };
  });

  // src/vendor/mod14.cjs
  var bt = n((Br, wt) => {
    "use strict";
    var Be = 14;
    function De(t) {
      return t * 2 + Number(Be);
    }
    function Ge() {
      return "mod14:";
    }
    wt.exports = { twice: De, tag: Ge };
  });

  // src/vendor/mod15.cjs
  var vt = n((Dr, $t) => {
    "use strict";
    var He = 15;
    function Ke(t) {
      return t * 2 + Number(He);
    }
    function Qe() {
      return "mod15:";
    }
    $t.exports = { twice: Ke, tag: Qe };
  });

  // src/vendor/mod16.cjs
  var ht = n((Gr, xt) => {
    "use strict";
    var Ue = 16;
    function Ve(t) {
      return t * 2 + Number(Ue);
    }
    function We() {
      return "mod16:";
    }
    xt.exports = { twice: Ve, tag: We };
  });

  // src/vendor/mod17.cjs
  var Nt = n((Hr, lt) => {
    "use strict";
    var Ye = 17;
    function Ze(t) {
      return t * 2 + Number(Ye);
    }
    function tr() {
      return "mod17:";
    }
    lt.exports = { twice: Ze, tag: tr };
  });

  // src/vendor/mod18.cjs
  var St = n((Kr, yt) => {
    "use strict";
    var er = 18;
    function rr(t) {
      return t * 2 + Number(er);
    }
    function nr() {
      return "mod18:";
    }
    yt.exports = { twice: rr, tag: nr };
  });

  // src/vendor/mod19.cjs
  var kt = n((Qr, Mt) => {
    "use strict";
    var or = 19;
    function ir(t) {
      return t * 2 + Number(or);
    }
    function ur() {
      return "mod19:";
    }
    Mt.exports = { twice: ir, tag: ur };
  });

  // src/vendor/mod20.cjs
  var Et = n((Ur, At) => {
    "use strict";
    var sr = 20;
    function ar(t) {
      return t * 2 + Number(sr);
    }
    function cr() {
      return "mod20:";
    }
    At.exports = { twice: ar, tag: cr };
  });

  // src/vendor/mod21.cjs
  var Lt = n((Vr, Ft) => {
    "use strict";
    var mr = 21;
    function pr(t) {
      return t * 2 + Number(mr);
    }
    function fr() {
      return "mod21:";
    }
    Ft.exports = { twice: pr, tag: fr };
  });

  // src/vendor/mod22.cjs
  var Pt = n((Wr, Rt) => {
    "use strict";
    var dr = 22;
    function gr(t) {
      return t * 2 + Number(dr);
    }
    function wr() {
      return "mod22:";
    }
    Rt.exports = { twice: gr, tag: wr };
  });

  // src/vendor/mod23.cjs
  var Ct = n((Yr, qt) => {
    "use strict";
    var br = 23;
    function $r(t) {
      return t * 2 + Number(br);
    }
    function vr() {
      return "mod23:";
    }
    qt.exports = { twice: $r, tag: vr };
  });

  // node_modules/ms/index.js
  var Ot = n((tn, Jt) => {
    var a = 1e3;
    var c = a * 60;
    var m = c * 60;
    var s = m * 24;
    var xr = s * 7;
    var hr = s * 365.25;
    Jt.exports = function(t, e) {
      e = e || {};
      var r = typeof t;
      if (r === "string" && t.length > 0) {
        return lr(t);
      } else if (r === "number" && isFinite(t)) {
        return e.long ? yr(t) : Nr(t);
      }
      throw new Error(
        "val is not a non-empty string or a valid number. val=" + JSON.stringify(t)
      );
    };
    function lr(t) {
      t = String(t);
      if (t.length > 100) {
        return;
      }
      var e = /^(-?(?:\d+)?\.?\d+) *(milliseconds?|msecs?|ms|seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h|days?|d|weeks?|w|years?|yrs?|y)?$/i.exec(
        t
      );
      if (!e) {
        return;
      }
      var r = parseFloat(e[1]);
      var i = (e[2] || "ms").toLowerCase();
      switch (i) {
        case "years":
        case "year":
        case "yrs":
        case "yr":
        case "y":
          return r * hr;
        case "weeks":
        case "week":
        case "w":
          return r * xr;
        case "days":
        case "day":
        case "d":
          return r * s;
        case "hours":
        case "hour":
        case "hrs":
        case "hr":
        case "h":
          return r * m;
        case "minutes":
        case "minute":
        case "mins":
        case "min":
        case "m":
          return r * c;
        case "seconds":
        case "second":
        case "secs":
        case "sec":
        case "s":
          return r * a;
        case "milliseconds":
        case "millisecond":
        case "msecs":
        case "msec":
        case "ms":
          return r;
        default:
          return void 0;
      }
    }
    function Nr(t) {
      var e = Math.abs(t);
      if (e >= s) {
        return Math.round(t / s) + "d";
      }
      if (e >= m) {
        return Math.round(t / m) + "h";
      }
      if (e >= c) {
        return Math.round(t / c) + "m";
      }
      if (e >= a) {
        return Math.round(t / a) + "s";
      }
      return t + "ms";
    }
    function yr(t) {
      var e = Math.abs(t);
      if (e >= s) {
        return f(t, e, s, "day");
      }
      if (e >= m) {
        return f(t, e, m, "hour");
      }
      if (e >= c) {
        return f(t, e, c, "minute");
      }
      if (e >= a) {
        return f(t, e, a, "second");
      }
      return t + " ms";
    }
    function f(t, e, r, i) {
      var u = e >= r * 1.5;
      return Math.round(t / r) + " " + i + (u ? "s" : "");
    }
  });

  // src/late.js
  var _t = {};
  X(_t, {
    LATE_PREFIX: () => Tt,
    late: () => Sr
  });
  function Sr(t) {
    return Tt + String(t * 2) + "/" + Xt.length;
  }
  var Tt, Xt;
  var jt = T(() => {
    Tt = "late#";
    Xt = [];
    for (let t = 0; t < 3; t++) {
      Xt.push(t * 2);
    }
  });

  // src/legacy.cjs
  var Bt = n((en, zt) => {
    function Mr(t) {
      const { late: e } = (jt(), te(_t));
      return e(t);
    }
    zt.exports = { lateReport: Mr };
  });

  // src/lazy.js
  var Dt = {};
  X(Dt, {
    stamp: () => kr
  });
  function kr(t) {
    return "stamp#" + String(t).padStart(3, "0");
  }
  var Gt = T(() => {
  });

  // src/main.js
  var d = o(z());
  var g = o(D());
  var w = o(H());
  var b = o(Q());
  var $ = o(V());
  var v = o(Y());
  var x = o(tt());
  var h = o(rt());
  var l = o(ot());
  var N = o(ut());
  var y = o(at());
  var S = o(mt());
  var M = o(ft());
  var k = o(gt());
  var A = o(bt());
  var E = o(vt());
  var F = o(ht());
  var L = o(Nt());
  var R = o(St());
  var P = o(kt());
  var q = o(Et());
  var C = o(Lt());
  var I = o(Pt());
  var J = o(Ct());

  // src/util.js
  function It(t) {
    return String(t).padStart(2, "0");
  }

  // src/main.js
  var O = o(Ot());
  var Ht = Zt("node:path");
  var Kt = o(Bt());
  async function Ar() {
    const { stamp: t } = await Promise.resolve().then(() => (Gt(), Dt));
    const e = [
      `dep00: ${d.default.tag()}${d.default.twice(21)}`,
      `dep01: ${g.default.tag()}${g.default.twice(21)}`,
      `dep02: ${w.default.tag()}${w.default.twice(21)}`,
      `dep03: ${b.default.tag()}${b.default.twice(21)}`,
      `dep04: ${$.default.tag()}${$.default.twice(21)}`,
      `dep05: ${v.default.tag()}${v.default.twice(21)}`,
      `dep06: ${x.default.tag()}${x.default.twice(21)}`,
      `dep07: ${h.default.tag()}${h.default.twice(21)}`,
      `dep08: ${l.default.tag()}${l.default.twice(21)}`,
      `dep09: ${N.default.tag()}${N.default.twice(21)}`,
      `dep10: ${y.default.tag()}${y.default.twice(21)}`,
      `dep11: ${S.default.tag()}${S.default.twice(21)}`,
      `dep12: ${M.default.tag()}${M.default.twice(21)}`,
      `dep13: ${k.default.tag()}${k.default.twice(21)}`,
      `dep14: ${A.default.tag()}${A.default.twice(21)}`,
      `dep15: ${E.default.tag()}${E.default.twice(21)}`,
      `dep16: ${F.default.tag()}${F.default.twice(21)}`,
      `dep17: ${L.default.tag()}${L.default.twice(21)}`,
      `dep18: ${R.default.tag()}${R.default.twice(21)}`,
      `dep19: ${P.default.tag()}${P.default.twice(21)}`,
      `dep20: ${q.default.tag()}${q.default.twice(21)}`,
      `dep21: ${C.default.tag()}${C.default.twice(21)}`,
      `dep22: ${I.default.tag()}${I.default.twice(21)}`,
      `dep23: ${J.default.tag()}${J.default.twice(21)}`,
      `pad: ${It(3)}`,
      `ms: ${(0, O.default)(9e4)} ${(0, O.default)("2h")}`,
      `path: ${(0, Ht.basename)("/var/log/report.txt")}`,
      `late: ${Kt.default.lateReport(21)}`,
      `stamp: ${t(7)}`
    ];
    for (const r of e) {
      console.log(r);
    }
  }
  Ar();
})();
