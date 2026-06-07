// Cross-runtime: finally overrides or preserves control flow.
function a() {
  try { return "try"; } finally { return "finally"; }
}
function b() {
  let x = "";
  try { x += "try"; throw new Error("boom"); }
  catch (_) { x += ":catch"; return x; }
  finally { x += ":finally"; }
}
function c() {
  for (let i = 0; i < 3; i++) {
    try { if (i === 1) break; }
    finally { console.log("fin" + i); }
  }
  return "done";
}

console.log(a());
console.log(b());
console.log(c());
