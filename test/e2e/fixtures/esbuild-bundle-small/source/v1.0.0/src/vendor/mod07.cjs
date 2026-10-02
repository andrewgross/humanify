"use strict";
var base = 7;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod07:";
}

module.exports = { twice, tag };
