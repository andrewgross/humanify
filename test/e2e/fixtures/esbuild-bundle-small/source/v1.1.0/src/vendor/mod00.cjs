"use strict";
var base = 0;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod00:";
}

module.exports = { twice, tag };
