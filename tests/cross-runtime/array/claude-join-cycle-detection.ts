// ONE thing: join/toString cycle detection. An array reachable from itself
// contributes an EMPTY string instead of recursing — the spec keeps a stack of
// arrays already being joined.
const a: any[] = [1, 2];
a.push(a);
console.log("self=" + a.join(","));
console.log("selfToString=" + String(a));

const b: any[] = [1];
const c: any[] = [2, b];
b.push(c);
console.log("mutual_b=" + b.join("-"));
console.log("mutual_c=" + c.join("-"));

const d: any[] = [];
d.push(d, d, 9);
console.log("triple=" + d.join("|"));

// The cycle guard is per-CALL, not permanent.
console.log("again=" + a.join(","));

// A cycle through a plain object is NOT guarded.
const o: any = { toString() { return "O"; } };
console.log("obj=" + [o, 1].join(","));

console.log("nested=" + [1, [2, [3, [4]]]].join(","));
console.log("nestedStr=" + String([1, [2, [3, [4]]]]));
