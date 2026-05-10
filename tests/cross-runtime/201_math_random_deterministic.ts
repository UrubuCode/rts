// Cross-runtime: Math.random properties (not values)
const r1 = Math.random();
const r2 = Math.random();

console.log("range=" + (r1 >= 0 && r1 < 1));
console.log("different=" + (r1 !== r2));
console.log("type=" + typeof r1);

// Multiple calls
let allInRange = true;
for (let i = 0; i < 10; i++) {
  const r = Math.random();
  if (r < 0 || r >= 1) {
    allInRange = false;
  }
}
console.log("all_in_range=" + allInRange);
