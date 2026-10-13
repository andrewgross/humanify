(() => {
  var Ut = Object.create;
  var p = Object.defineProperty;
  var Vt = Object.getOwnPropertyDescriptor;
  var Wt = Object.getOwnPropertyNames;
  var Yt = Object.getPrototypeOf;
  var Zt = Object.prototype.hasOwnProperty;
  var te = /* @__PURE__ */ ((t) => typeof require !== "undefined" ? require : typeof Proxy !== "undefined" ? new Proxy(t, {
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
      for (let u of Wt(e))
        if (!Zt.call(t, u) && u !== r)
          p(t, u, { get: () => e[u], enumerable: !(i = Vt(e, u)) || i.enumerable });
    }
    return t;
  };
  var o = (t, e, r) => (r = t != null ? Ut(Yt(t)) : {}, _(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    e || !t || !t.__esModule ? p(r, "default", { value: t, enumerable: true }) : r,
    t
  ));
  var ee = (t) => _(p({}, "__esModule", { value: true }), t);

  // src/vendor/mod00.cjs
  var z = n((Lr, j) => {
    "use strict";
    var re = 0;
    function ne(t) {
      return t * 2 + Number(re);
    }
    function oe() {
      return "mod00:";
    }
    j.exports = { twice: ne, tag: oe };
  });

  // src/vendor/mod01.cjs
  var D = n((Rr, B) => {
    "use strict";
    var ie = 1;
    function ue(t) {
      return t * 2 + Number(ie);
    }
    function se() {
      return "mod01:";
    }
    B.exports = { twice: ue, tag: se };
  });

  // src/vendor/mod02.cjs
  var H = n((Pr, G) => {
    "use strict";
    var ae = 2;
    function ce(t) {
      return t * 2 + Number(ae);
    }
    function me() {
      return "mod02:";
    }
    G.exports = { twice: ce, tag: me };
  });

  // src/vendor/mod03.cjs
  var Q = n((qr, K) => {
    "use strict";
    var pe = 3;
    function de(t) {
      return t * 2 + Number(pe);
    }
    function fe() {
      return "mod03:";
    }
    K.exports = { twice: de, tag: fe };
  });

  // src/vendor/mod04.cjs
  var V = n((Cr, U) => {
    "use strict";
    var ge = 4;
    function we(t) {
      return t * 2 + Number(ge);
    }
    function be() {
      return "mod04:";
    }
    U.exports = { twice: we, tag: be };
  });

  // src/vendor/mod05.cjs
  var Y = n((Ir, W) => {
    "use strict";
    var $e = 5;
    function ve(t) {
      return t * 2 + Number($e);
    }
    function xe() {
      return "mod05:";
    }
    W.exports = { twice: ve, tag: xe };
  });

  // src/vendor/mod06.cjs
  var tt = n((Jr, Z) => {
    "use strict";
    var le = 6;
    function he(t) {
      return t * 2 + Number(le);
    }
    function Ne() {
      return "mod06:";
    }
    Z.exports = { twice: he, tag: Ne };
  });

  // src/vendor/mod07.cjs
  var rt = n((Or, et) => {
    "use strict";
    var ye = 7;
    function Se(t) {
      return t * 2 + Number(ye);
    }
    function Me() {
      return "mod07:";
    }
    et.exports = { twice: Se, tag: Me };
  });

  // src/vendor/mod08.cjs
  var ot = n((Tr, nt) => {
    "use strict";
    var ke = 8;
    function Ae(t) {
      return t * 2 + Number(ke);
    }
    function Ee() {
      return "mod08:";
    }
    nt.exports = { twice: Ae, tag: Ee };
  });

  // src/vendor/mod09.cjs
  var ut = n((Xr, it) => {
    "use strict";
    var Fe = 9;
    function Le(t) {
      return t * 2 + Number(Fe);
    }
    function Re() {
      return "mod09:";
    }
    it.exports = { twice: Le, tag: Re };
  });

  // src/vendor/mod10.cjs
  var at = n((_r, st) => {
    "use strict";
    var Pe = 10;
    function qe(t) {
      return t * 2 + Number(Pe);
    }
    function Ce() {
      return "mod10:";
    }
    st.exports = { twice: qe, tag: Ce };
  });

  // src/vendor/mod11.cjs
  var mt = n((jr, ct) => {
    "use strict";
    var Ie = 11;
    function Je(t) {
      return t * 2 + Number(Ie);
    }
    function Oe() {
      return "mod11:";
    }
    ct.exports = { twice: Je, tag: Oe };
  });

  // src/vendor/mod12.cjs
  var dt = n((zr, pt) => {
    "use strict";
    var Te = 12;
    function Xe(t) {
      return t * 2 + Number(Te);
    }
    function _e() {
      return "mod12:";
    }
    pt.exports = { twice: Xe, tag: _e };
  });

  // src/vendor/mod13.cjs
  var gt = n((Br, ft) => {
    "use strict";
    var je = 13;
    function ze(t) {
      return t * 2 + Number(je);
    }
    function Be() {
      return "mod13:";
    }
    ft.exports = { twice: ze, tag: Be };
  });

  // src/vendor/mod14.cjs
  var bt = n((Dr, wt) => {
    "use strict";
    var De = 14;
    function Ge(t) {
      return t * 2 + Number(De);
    }
    function He() {
      return "mod14:";
    }
    wt.exports = { twice: Ge, tag: He };
  });

  // src/vendor/mod15.cjs
  var vt = n((Gr, $t) => {
    "use strict";
    var Ke = 15;
    function Qe(t) {
      return t * 2 + Number(Ke);
    }
    function Ue() {
      return "mod15:";
    }
    $t.exports = { twice: Qe, tag: Ue };
  });

  // src/vendor/mod16.cjs
  var lt = n((Hr, xt) => {
    "use strict";
    var Ve = 16;
    function We(t) {
      return t * 2 + Number(Ve);
    }
    function Ye() {
      return "mod16:";
    }
    xt.exports = { twice: We, tag: Ye };
  });

  // src/vendor/mod17.cjs
  var Nt = n((Kr, ht) => {
    "use strict";
    var Ze = 17;
    function tr(t) {
      return t * 2 + Number(Ze);
    }
    function er() {
      return "mod17:";
    }
    ht.exports = { twice: tr, tag: er };
  });

  // src/vendor/mod18.cjs
  var St = n((Qr, yt) => {
    "use strict";
    var rr = 18;
    function nr(t) {
      return t * 2 + Number(rr);
    }
    function or() {
      return "mod18:";
    }
    yt.exports = { twice: nr, tag: or };
  });

  // src/vendor/mod19.cjs
  var kt = n((Ur, Mt) => {
    "use strict";
    var ir = 19;
    function ur(t) {
      return t * 2 + Number(ir);
    }
    function sr() {
      return "mod19:";
    }
    Mt.exports = { twice: ur, tag: sr };
  });

  // src/vendor/mod20.cjs
  var Et = n((Vr, At) => {
    "use strict";
    var ar = 20;
    function cr(t) {
      return t * 2 + Number(ar);
    }
    function mr() {
      return "mod20:";
    }
    At.exports = { twice: cr, tag: mr };
  });

  // src/vendor/mod21.cjs
  var Lt = n((Wr, Ft) => {
    "use strict";
    var pr = 21;
    function dr(t) {
      return t * 2 + Number(pr);
    }
    function fr() {
      return "mod21:";
    }
    Ft.exports = { twice: dr, tag: fr };
  });

  // src/vendor/mod22.cjs
  var Pt = n((Yr, Rt) => {
    "use strict";
    var gr = 22;
    function wr(t) {
      return t * 2 + Number(gr);
    }
    function br() {
      return "mod22:";
    }
    Rt.exports = { twice: wr, tag: br };
  });

  // src/vendor/mod23.cjs
  var It = n((Zr, Ct) => {
    "use strict";
    var qt = 23;
    function $r(t) {
      return t * 2 + Number(qt);
    }
    function vr() {
      return "mod23:";
    }
    function xr(t) {
      return t * 3 + Number(qt);
    }
    Ct.exports = { twice: $r, tag: vr, tripled: xr };
  });

  // node_modules/ms/index.js
  var Tt = n((en, Ot) => {
    var a = 1e3;
    var c = a * 60;
    var m = c * 60;
    var s = m * 24;
    var lr = s * 7;
    var hr = s * 365.25;
    Ot.exports = function(t, e) {
      e = e || {};
      var r = typeof t;
      if (r === "string" && t.length > 0) {
        return Nr(t);
      } else if (r === "number" && isFinite(t)) {
        return e.long ? Sr(t) : yr(t);
      }
      throw new Error(
        "val is not a non-empty string or a valid number. val=" + JSON.stringify(t)
      );
    };
    function Nr(t) {
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
          return r * lr;
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
    function yr(t) {
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
    function Sr(t) {
      var e = Math.abs(t);
      if (e >= s) {
        return d(t, e, s, "day");
      }
      if (e >= m) {
        return d(t, e, m, "hour");
      }
      if (e >= c) {
        return d(t, e, c, "minute");
      }
      if (e >= a) {
        return d(t, e, a, "second");
      }
      return t + " ms";
    }
    function d(t, e, r, i) {
      var u = e >= r * 1.5;
      return Math.round(t / r) + " " + i + (u ? "s" : "");
    }
  });

  // src/late.js
  var jt = {};
  X(jt, {
    LATE_PREFIX: () => Xt,
    late: () => Mr
  });
  function Mr(t) {
    return Xt + String(t * 2).padStart(4, "0") + "/" + _t.length;
  }
  var Xt, _t;
  var zt = T(() => {
    Xt = "late#";
    _t = [];
    for (let t = 0; t < 3; t++) {
      _t.push(t * 2);
    }
  });

  // src/legacy.cjs
  var Dt = n((rn, Bt) => {
    function kr(t) {
      const { late: e } = (zt(), ee(jt));
      return e(t);
    }
    Bt.exports = { lateReport: kr };
  });

  // src/lazy.js
  var Gt = {};
  X(Gt, {
    stamp: () => Ar
  });
  function Ar(t) {
    return "stamp#" + String(t).padStart(3, "0");
  }
  var Ht = T(() => {
  });

  // src/main.js
  var g = o(z());
  var w = o(D());
  var b = o(H());
  var $ = o(Q());
  var v = o(V());
  var x = o(Y());
  var l = o(tt());
  var h = o(rt());
  var N = o(ot());
  var y = o(ut());
  var S = o(at());
  var M = o(mt());
  var k = o(dt());
  var A = o(gt());
  var E = o(bt());
  var F = o(vt());
  var L = o(lt());
  var R = o(Nt());
  var P = o(St());
  var q = o(kt());
  var C = o(Et());
  var I = o(Lt());
  var J = o(Pt());
  var f = o(It());

  // src/util.js
  function Jt(t) {
    return String(t).padStart(2, "0");
  }

  // src/main.js
  var O = o(Tt());
  var Kt = te("node:path");
  var Qt = o(Dt());
  async function Er() {
    const { stamp: t } = await Promise.resolve().then(() => (Ht(), Gt));
    const e = [
      `dep00: ${g.default.tag()}${g.default.twice(21)}`,
      `dep01: ${w.default.tag()}${w.default.twice(21)}`,
      `dep02: ${b.default.tag()}${b.default.twice(21)}`,
      `dep03: ${$.default.tag()}${$.default.twice(21)}`,
      `dep04: ${v.default.tag()}${v.default.twice(21)}`,
      `dep05: ${x.default.tag()}${x.default.twice(21)}`,
      `dep06: ${l.default.tag()}${l.default.twice(21)}`,
      `dep07: ${h.default.tag()}${h.default.twice(21)}`,
      `dep08: ${N.default.tag()}${N.default.twice(21)}`,
      `dep09: ${y.default.tag()}${y.default.twice(21)}`,
      `dep10: ${S.default.tag()}${S.default.twice(21)}`,
      `dep11: ${M.default.tag()}${M.default.twice(21)}`,
      `dep12: ${k.default.tag()}${k.default.twice(21)}`,
      `dep13: ${A.default.tag()}${A.default.twice(21)}`,
      `dep14: ${E.default.tag()}${E.default.twice(21)}`,
      `dep15: ${F.default.tag()}${F.default.twice(21)}`,
      `dep16: ${L.default.tag()}${L.default.twice(21)}`,
      `dep17: ${R.default.tag()}${R.default.twice(21)}`,
      `dep18: ${P.default.tag()}${P.default.twice(21)}`,
      `dep19: ${q.default.tag()}${q.default.twice(21)}`,
      `dep20: ${C.default.tag()}${C.default.twice(21)}`,
      `dep21: ${I.default.tag()}${I.default.twice(21)}`,
      `dep22: ${J.default.tag()}${J.default.twice(21)}`,
      `dep23: ${f.default.tag()}${f.default.twice(21)}`,
      `dep23 tripled: ${f.default.tripled(7)}`,
      `pad: ${Jt(3)}`,
      `ms: ${(0, O.default)(9e4)} ${(0, O.default)("2h")}`,
      `path: ${(0, Kt.basename)("/var/log/report.txt")}`,
      `late: ${Qt.default.lateReport(21)}`,
      `stamp: ${t(9)}`
    ];
    for (const r of e) {
      console.log(r);
    }
  }
  Er();
})();
