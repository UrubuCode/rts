// Cross-runtime: a cleared Map can be reused with fresh insertion order.
const m = new Map([["old", 1], ["older", 2]]);
m.clear();
console.log(m.size, [...m].length, m.has("old"));
m.set("new", 3).set("next", 4);
console.log(JSON.stringify([...m]));

