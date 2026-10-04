"use strict";
var base = 20;
function twice(x) {
  return x * 2 + Number(base);
}
function tag() {
  return "mod20:";
}

module.exports = { twice, tag };
