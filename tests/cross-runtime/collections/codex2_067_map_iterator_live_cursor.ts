// Cross-runtime: a Map iterator continues from its live cursor.
const m = new Map([["a", 1], ["b", 2]]);
const it = m.entries();
console.log(JSON.stringify(it.next()));
m.set("c", 3);
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));

