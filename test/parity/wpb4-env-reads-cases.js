// WPB.4 env-reads gate corpus: every Babel shape the oxc translation must
// reproduce. Both binaries' reports over this file are byte-compared by
// test/parity/wpb4-env-reads.sh. Its layout is the input: not formatted.
process.env.PLAIN;
process.env["STR"];
process.env['SINGLE'];
process.env[`TPL`];
process.env[1];
process.env[key];
process.env[a + b];
process?.env.OPT_BASE;
process.env?.OPT_PARENT;
process.env.OPT_LATER?.x;
process.env?.["OPT_COMPUTED"];
(process.env).PAREN;
((process.env))["PAREN2"];
Bun.env.BUN;
import.meta.env.META;
import.meta.env?.META_OPT;
const { D1, D2: renamed, "D3": d3, ["D4"]: d4, [dyn]: d5, 5: d6, ...restAll } = process.env;
const { E1 = "default" } = process.env;
const [arrayPattern] = process.env;
const alias = process.env;
alias.ALIAS_READ;
alias["ALIAS_STR"];
alias?.ALIAS_OPT;
const { ALIAS_DESTRUCT } = alias;
const alias2 = alias;
alias2.CHAINED;
let reassigned = process.env;
reassigned = {};
reassigned.AFTER_REASSIGN;
reassigned++;
Object.keys(process.env);
const spread = { ...process.env };
fn(process.env);
process.env.WRITE = "x";
delete process.env.DELETED;
process.env = {};
typeof process.env.TYPEOF;
x[process.env] = 1;
function shadowed(process) {
  return process.env.SHADOWED;
}
function shadowedBun() {
  var Bun = {};
  return Bun.env.SHADOWED_BUN;
}
class K {
  m() {
    return process.env.IN_METHOD;
  }
}
const arrow = () => process.env.IN_ARROW;
/* multi
line */ process.env.AFTER_COMMENT;
const longSnippet = process.env[
  "a very long computed key expression that goes on and on" + "and on and on well past eighty units"
];
const unicode = "é😀"; process.env.AFTER_UNICODE;
if (process.env.NODE_ENV === "production") {}
process.env.aLower; process.env.A_UPPER; process.env._UNDER; process.env.$DOLLAR; process.env.Z9; process.env.a_b; process.env.ab;
