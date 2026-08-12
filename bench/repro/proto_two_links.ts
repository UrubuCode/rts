class Base { bp(): number { return 1; } }
class Derived extends Base { x: number = 1; }
const derived = new Derived();
let a = 0;
for (let i = 0; i < 400000; i++) a += derived.bp();
console.log(a);
