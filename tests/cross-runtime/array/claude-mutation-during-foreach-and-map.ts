// ONE thing: the iteration methods capture length ONCE and read each index
// live, so an element appended during the walk is never visited and one
// deleted before it is reached is skipped.
const appended: number[] = [];
const a = [1, 2, 3];
a.forEach((v) => { appended.push(v); if (v === 1) a.push(99); });
console.log("append=" + appended.join(",") + " finalLen=" + a.length);

const deleted: string[] = [];
const b = [1, 2, 3, 4];
b.forEach((v, i) => { deleted.push(String(v)); if (i === 0) delete b[2]; });
console.log("delete=" + deleted.join(","));

const shrunk: string[] = [];
const c = [1, 2, 3, 4, 5];
c.forEach((v) => { shrunk.push(String(v)); if (v === 2) c.length = 3; });
console.log("shrink=" + shrunk.join(",") + " len=" + c.length);

const overwritten: string[] = [];
const d = [1, 2, 3];
d.forEach((v, i) => { overwritten.push(String(v)); if (i === 0) d[2] = 77; });
console.log("overwrite=" + overwritten.join(","));

// map allocates the result with the ORIGINAL length.
const e = [1, 2, 3];
const mapped = e.map((v, i) => { if (i === 0) e.push(9); return v; });
console.log("map=" + mapped.length + " src=" + e.length);

// filter reads the same window.
const f = [1, 2, 3];
const filtered = f.filter((v, i) => { if (i === 0) f.push(9); return true; });
console.log("filter=" + filtered.join(","));

// reduce with no initial value takes the first PRESENT element as the seed.
const g: any[] = [, , 5, 6];
console.log("reduceHoles=" + g.reduce((x, y) => x + y));

// find/findIndex/findLast read holes as undefined and always walk every index.
const h: any[] = [1, , 3];
let probes = 0;
h.find(() => { probes++; return false; });
console.log("findProbes=" + probes);
console.log("findUndef=" + String(h.find((v) => v === undefined)));
console.log("findIndexUndef=" + h.findIndex((v) => v === undefined));
console.log("findLastIndex=" + h.findLastIndex((v) => v === undefined));

// some/every stop at the first decision.
let someProbes = 0;
[1, 2, 3].some((v) => { someProbes++; return v === 2; });
console.log("someProbes=" + someProbes);
let everyProbes = 0;
[1, 2, 3].every((v) => { everyProbes++; return v < 2; });
console.log("everyProbes=" + everyProbes);
console.log("emptyEvery=" + ([] as number[]).every(() => false) + " emptySome=" + ([] as number[]).some(() => true));
