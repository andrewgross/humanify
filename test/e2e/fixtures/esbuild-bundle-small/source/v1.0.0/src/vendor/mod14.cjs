"use strict";
var base = 14;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod14:";
}

module.exports = { twice, tag };
