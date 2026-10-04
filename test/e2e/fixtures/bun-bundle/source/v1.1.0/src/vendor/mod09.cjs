"use strict";
var base = 9;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod09:";
}

module.exports = { twice, tag };
