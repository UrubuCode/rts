// ONE thing: fill writes the SAME reference into every slot (not a copy), and
// how its three indices clamp.
const objs = new Array(3).fill({ n: 0 });
objs[0].n = 9;
console.log("shared=" + objs.map((o: any) => o.n).join(",") + " same=" + (objs[0] === objs[2]));

const a = () => [0, 1, 2, 3, 4];
console.log("all=" + a().fill(9).join(","));
console.log("from2=" + a().fill(9, 2).join(","));
console.log("fromNeg2=" + a().fill(9, -2).join(","));
console.log("two_four=" + a().fill(9, 2, 4).join(","));
console.log("neg=" + a().fill(9, -3, -1).join(","));
console.log("startGtEnd=" + a().fill(9, 4, 2).join(","));
console.log("beyond=" + a().fill(9, 10, 20).join(","));
console.log("nan=" + a().fill(9, NaN, NaN).join(","));
console.log("undefEnd=" + a().fill(9, 1, undefined).join(","));
console.log("frac=" + a().fill(9, 1.7, 3.9).join(","));

const holes: any[] = [1, , 3, , 5];
holes.fill(0, 1, 4);
console.log("holesIn=" + [0, 1, 2, 3, 4].map((i) => (i in holes ? "y" : "n")).join("") + " v=" + holes.map(String).join(","));

const orig = [1, 2];
console.log("identity=" + (orig.fill(0) === orig));

const like: any = { length: 3 };
Array.prototype.fill.call(like, "z", 1);
console.log("like=" + [0, 1, 2].map((i) => String(like[i])).join(","));

try { Object.freeze([1, 2]).fill(0); } catch (e: any) { console.log("frozen=" + e.constructor.name); }
