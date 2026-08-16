// Cross-runtime: array iterators expose ordered result objects through exhaustion.
const it = ["a", "b"].values();
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));
console.log(JSON.stringify(it.next()));

