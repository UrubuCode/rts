// Cross-runtime: deleting and reinserting a Map key moves it to the end.
const m = new Map([["a", 1], ["b", 2], ["c", 3]]);
m.delete("b");
m.set("b", 22);
console.log(JSON.stringify([...m]));
console.log(m.size);

