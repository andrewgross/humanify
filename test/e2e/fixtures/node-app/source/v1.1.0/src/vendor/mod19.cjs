"use strict";
var base = 19;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod19:";
}

module.exports = { twice, tag };
