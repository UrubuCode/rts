// Cross-runtime: Map and Set size property
const map = new Map();
console.log("map_empty=" + map.size);

map.set("a", 1);
map.set("b", 2);
console.log("map_2=" + map.size);

map.set("a", 10);
console.log("map_still_2=" + map.size);

map.delete("a");
console.log("map_1=" + map.size);

map.clear();
console.log("map_cleared=" + map.size);

const set = new Set();
console.log("set_empty=" + set.size);

set.add(1);
set.add(2);
set.add(1);
console.log("set_2=" + set.size);

set.delete(1);
console.log("set_1=" + set.size);
