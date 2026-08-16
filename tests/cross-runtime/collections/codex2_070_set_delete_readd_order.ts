// Cross-runtime: deleting and re-adding a Set value moves it to the end.
const s = new Set([1, 2, 3, 4]);
s.delete(2);
s.add(2);
console.log([...s].join(","));
console.log(s.has(2), s.delete(9), s.size);

