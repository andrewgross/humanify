"use strict";
var base = 18;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod18:";
}

module.exports = { twice, tag };
