// Cross-runtime: Map and Set instance basics.
const m = new Map();
m.set("a", 1);
m.set("b", 2);
console.log("m_size=" + m.size);
console.log("m_get=" + m.get("a"));
console.log("m_has=" + m.has("b"));
m.delete("b");
console.log("m_after=" + m.size);

const s = new Set();
s.add("x");
s.add("y");
s.add("x");
console.log("s_size=" + s.size);
console.log("s_has=" + s.has("x"));
s.delete("x");
console.log("s_after=" + s.size);
