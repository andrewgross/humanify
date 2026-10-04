"use strict";
var base = 1;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod01:";
}

module.exports = { twice, tag };
