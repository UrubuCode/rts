let trace = [];
function t(name) { trace.push(name); return name.length; }
const config = { a: { b: 1 } };
const {
  a: { b = t("b"), c = t("c") } = {},
  d: { e = t("e") } = {},
} = config;
console.log(b, c, e);
console.log(trace.join(","));
trace = [];
const {
  m = t("m1"),
  n: { o = t("o") } = (trace.push("default-n"), {}),
} = { m: 5 };
console.log(m, o);
console.log(trace.join(","));
