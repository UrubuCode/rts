// Cross-runtime: array keys iteration includes every index in a sparse length.
const a: any[] = [];
a.length = 5;
a[2] = "x";
console.log([...a.keys()].join(","));
console.log([...a.values()].map(String).join(","));
console.log(JSON.stringify([...a.entries()]));

