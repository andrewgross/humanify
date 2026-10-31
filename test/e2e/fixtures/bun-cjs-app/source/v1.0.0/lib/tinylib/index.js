/*! tinylib v1.0.0 | MIT */
"use strict";
function shout(s) {
  return String(s).toUpperCase() + "!";
}
function pad(s, n) {
  s = String(s);
  while (s.length < n) s += ".";
  return "TINYLIB-MARK:" + s;
}
module.exports = { shout, pad };
