// Cross-runtime: subarray shares buffer and sort mutates the view.
const a = new Uint8Array([5, 4, 3, 2, 1]);
const sub = a.subarray(1, 4);
sub.sort();
console.log(Array.from(sub).join(","));
console.log(Array.from(a).join(","));
sub[0] = 9;
console.log(Array.from(a).join(","));
