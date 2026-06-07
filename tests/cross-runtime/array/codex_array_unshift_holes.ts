// Cross-runtime: unshift preserves holes and moves existing keys.
const a = [,, "x"] as any[];
a.unshift("a", "b");
console.log(a.length);
console.log(a.join("|"));
console.log(Object.keys(a).join(","));
console.log(2 in a, 3 in a, 4 in a);
