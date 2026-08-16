// Cross-runtime: deleting an unvisited Set value removes it from a live iterator.
const s = new Set(["a", "b", "c"]);
const it = s.values();
console.log(JSON.stringify(it.next()));
s.delete("b");
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));

