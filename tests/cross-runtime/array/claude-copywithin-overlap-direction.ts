// ONE thing: copyWithin over OVERLAPPING ranges. The spec copies in whichever
// direction preserves the source, which a naive forward loop gets wrong.
const a = () => [0, 1, 2, 3, 4, 5, 6, 7];
console.log("fwdOverlap=" + a().copyWithin(0, 3).join(","));
console.log("backOverlap=" + a().copyWithin(3, 0).join(","));
console.log("backBounded=" + a().copyWithin(3, 0, 5).join(","));
console.log("selfSame=" + a().copyWithin(2, 2).join(","));
console.log("shiftDown=" + a().copyWithin(1, 0).join(","));
console.log("shiftUp=" + a().copyWithin(0, 1).join(","));
console.log("negTarget=" + a().copyWithin(-3, 0, 3).join(","));
console.log("negStart=" + a().copyWithin(0, -3).join(","));
console.log("negEnd=" + a().copyWithin(0, 1, -1).join(","));
console.log("allNeg=" + a().copyWithin(-2, -4, -2).join(","));
console.log("outOfRange=" + a().copyWithin(20, 0).join(","));
console.log("startGtEnd=" + a().copyWithin(0, 5, 2).join(","));
console.log("frac=" + a().copyWithin(0.9, 3.9).join(","));

const h: any[] = [0, , 2, 3, 4];
h.copyWithin(3, 0, 2);
console.log("holeIn=" + [0, 1, 2, 3, 4].map((i) => (i in h ? "y" : "n")).join("") + " v=" + h.map(String).join(","));

const o = [1, 2, 3];
console.log("identity=" + (o.copyWithin(0, 1) === o) + " len=" + o.length);

const t = new Uint8Array([1, 2, 3, 4, 5]);
console.log("typed=" + Array.from(t.copyWithin(0, 2)).join(","));
