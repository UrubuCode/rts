// Cross-runtime: private class fields (#field syntax, ES2022).

class Counter {
  #count = 0;
  #step: number;

  constructor(step = 1) { this.#step = step; }

  increment() { this.#count += this.#step; }
  decrement() { this.#count -= this.#step; }
  reset()     { this.#count = 0; }
  get value() { return this.#count; }
}

const c = new Counter(2);
c.increment(); c.increment(); c.increment();
console.log(c.value);   // 6
c.decrement();
console.log(c.value);   // 4
c.reset();
console.log(c.value);   // 0

// private field not accessible outside
try {
  console.log((c as any)["#count"]);   // undefined (not same as #count)
} catch (_) {}
console.log("#count" in c);  // false — not a string key

// private static field
class IdGen {
  static #next = 1;
  readonly id: number;
  constructor() { this.id = IdGen.#next++; }
  static reset() { IdGen.#next = 1; }
}
const a = new IdGen(), b = new IdGen(), c2 = new IdGen();
console.log(a.id, b.id, c2.id);  // 1 2 3
IdGen.reset();
const d = new IdGen();
console.log(d.id);  // 1

// private field shadowing in subclass
class Base {
  #x = 10;
  getX() { return this.#x; }
}
class Child extends Base {
  #x = 20;  // separate field — does NOT shadow Base#x
  getChildX() { return this.#x; }
}
const ch = new Child();
console.log(ch.getX());       // 10 (Base#x)
console.log(ch.getChildX());  // 20 (Child#x)
