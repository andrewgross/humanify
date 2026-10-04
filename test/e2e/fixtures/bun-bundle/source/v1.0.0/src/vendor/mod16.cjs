"use strict";
var base = 16;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod16:";
}

module.exports = { twice, tag };
