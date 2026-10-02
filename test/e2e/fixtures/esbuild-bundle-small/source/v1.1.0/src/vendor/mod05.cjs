"use strict";
var base = 5;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod05:";
}

module.exports = { twice, tag };
