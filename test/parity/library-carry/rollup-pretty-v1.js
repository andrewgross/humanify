// app entry (scope-hoisted bundle)
var banner_text="qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
var cfg={debug:false,retries:3};
function a1(t){return t.trim().toLowerCase()}
var a2=function(e,n){return e.map(function(r){return r*n})};
const a3=(o)=>Object.keys(o).length;
var a4=function(s){return a1(s)+cfg.retries};

/*! lodash-lite v4.1.0 */
// ---- vendored module ----
function c1(r,s){var o=[];for(var i=0;i<r.length;i+=s){o.push(r.slice(i,i+s))}return o}
var c2=function(r){return r.filter(function(v){return !!v})};
var c3={k:function(o){return Object.keys(o)},v(o){return Object.values(o)}};

/*! @scope/tiny-emitter v2.0.1 */
// ---- vendored module ----
class E1{constructor(){this.h={}}on(e,f){(this.h[e]=this.h[e]||[]).push(f);return this}emit(e,...a){(this.h[e]||[]).forEach(function(f){f.apply(null,a)})}}
const e2=()=>new E1();

/*! date-fmt v0.3.0 */
// ---- vendored module ----
function p1(n){return n<10?"0"+n:""+n}
const p2=d=>p1(d.getHours())+":"+p1(d.getMinutes());
const p3=d=>p2(d)+":"+p1(d.getSeconds());
console.log(banner_text.length,cfg,a1,a2,a3,a4,c1,c2,c3,e2,p2,p3);
