// Cross-runtime: generator .return and .throw with finally.
function* g() {
  try { yield 1; yield 2; }
  finally { yield 99; }
}
const a = g();
const s1 = a.next();
const s2 = a.return(7);
const b = g();
b.next();
let thrown = "";
try { b.throw(new Error("boom")); } catch (e: any) { thrown = e.message; }
console.log("return=" + [s1.value, s1.done, s2.value, s2.done].join(","));
console.log("throw=" + thrown);
