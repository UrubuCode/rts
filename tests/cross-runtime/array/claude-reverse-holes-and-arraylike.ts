// ONE thing: reverse MOVES holes rather than filling them, so it deletes on one
// side while it writes on the other.
const h: any[] = [0, , 2, , 4];
h.reverse();
console.log("in=" + [0, 1, 2, 3, 4].map((i) => (i in h ? "y" : "n")).join("") + " v=" + h.map(String).join(","));

const odd: any[] = [0, , 2];
odd.reverse();
console.log("oddIn=" + [0, 1, 2].map((i) => (i in odd ? "y" : "n")).join("") + " v=" + odd.map(String).join(","));

const one: any[] = [, ];
one.reverse();
console.log("holeOnlyLen=" + one.length + " in0=" + (0 in one));

const a = [1, 2, 3];
console.log("identity=" + (a.reverse() === a) + " len=" + a.length + " v=" + a.join(","));

const t: any[] = [0, , 2];
const tr = t.toReversed();
console.log("toRevIn=" + [0, 1, 2].map((i) => (i in tr ? "y" : "n")).join("") + " v=" + tr.map(String).join(","));

const like: any = { length: 3, 0: "a", 2: "c" };
Array.prototype.reverse.call(like);
console.log("like=" + [0, 1, 2].map((i) => (i in like ? String(like[i]) : "hole")).join(","));

try { Array.prototype.reverse.call("abc" as any); }
catch (e: any) { console.log("onString=" + e.constructor.name); }
