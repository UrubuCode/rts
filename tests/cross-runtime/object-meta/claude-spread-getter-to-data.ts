let calls = 0;
const src = {
  get a() { calls++; return 1; },
  get b() { calls++; return 2; },
};
const copy = { ...src };
console.log(copy.a);
console.log(copy.b);
console.log(calls);
const d = Object.getOwnPropertyDescriptor(copy, "a");
console.log(d.get === undefined);
console.log(d.value);
console.log(d.writable, d.enumerable, d.configurable);
