// Cross-runtime: WeakMap and WeakSet basic behavior.

// WeakMap
const wm = new WeakMap();
const k1 = {};
const k2 = {};
wm.set(k1, "value1");
wm.set(k2, 42);
console.log(wm.has(k1));
console.log(wm.get(k1));
console.log(wm.has(k2));
console.log(wm.get(k2));
wm.delete(k1);
console.log(wm.has(k1));

// WeakMap as private data store (classic pattern)
const _private = new WeakMap<object, { name: string; age: number }>();
class Person {
  constructor(name: string, age: number) {
    _private.set(this, { name, age });
  }
  greet() {
    const d = _private.get(this)!;
    return "Hi, I'm " + d.name + ", age " + d.age;
  }
}
const person = new Person("Alice", 30);
console.log(person.greet());
console.log(_private.has(person));

// WeakSet
const ws = new WeakSet();
const o1 = { id: 1 };
const o2 = { id: 2 };
ws.add(o1);
console.log(ws.has(o1));
console.log(ws.has(o2));
ws.delete(o1);
console.log(ws.has(o1));
