// Cross-runtime: own properties shadow a live prototype chain.
const root = { x: 1, y: 2 };
const mid = Object.create(root);
mid.x = 3;
const leaf = Object.create(mid);
leaf.z = 4;
console.log(leaf.x, leaf.y, leaf.z);
console.log(Object.hasOwn(leaf, "x"), "x" in leaf);
delete mid.x;
console.log(leaf.x);

