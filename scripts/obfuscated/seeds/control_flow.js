// Loops, labels, switch fallthrough, try/catch/finally, short-circuit.
const out = [];
outer: for (let i = 0; i < 4; i++) {
  for (let j = 0; j < 4; j++) {
    if (j === 2) continue outer;
    if (i === 3) break outer;
    out.push(i + "" + j);
  }
}
function sw(n) {
  let r = "";
  switch (n) {
    case 0: r += "z";
    case 1: r += "o"; break;
    case 2: r += "t"; break;
    default: r += "d";
  }
  return r;
}
out.push(sw(0) + sw(1) + sw(2) + sw(9));
function tf(n) {
  try {
    if (n === 0) throw new RangeError("zero");
    return "ok" + n;
  } catch (e) { return e.constructor.name; }
  finally { out.push("fin" + n); }
}
out.push(tf(0), tf(1));
let a = null;
out.push(a ?? "dflt", a?.deep?.er, 0 || "falsy", 1 && "truthy");
let n = 0;
do { n += 2; } while (n < 7);
out.push(n);
console.log(out.join("|"));
