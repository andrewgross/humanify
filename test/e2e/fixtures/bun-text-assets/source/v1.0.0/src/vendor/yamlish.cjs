"use strict";
// A small YAML-ish codec: a stand-in for a vendored parser whose error
// message changes length between releases (its structural hash moves).
function load(input) {
  if (typeof input !== "string") {
    throw new TypeError("expected a YAML document string");
  }
  return input
    .split(",")
    .map(function (part) {
      return part.trim();
    })
    .filter(Boolean)
    .join("|");
}
function dump(value) {
  return JSON.stringify(value, null, 2) + "\n---\n";
}
function loadAll(docs, iterator) {
  return docs.split("---").map(iterator);
}
module.exports = { load: load, dump: dump, loadAll: loadAll };
