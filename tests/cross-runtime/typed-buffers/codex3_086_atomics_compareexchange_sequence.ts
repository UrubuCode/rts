// Cross-runtime: Atomics compareExchange returns the old value and writes conditionally.
const shared = new SharedArrayBuffer(16);
const values = new Int32Array(shared);
values[0] = 5;
console.log(Atomics.compareExchange(values, 0, 4, 9), values[0]);
console.log(Atomics.compareExchange(values, 0, 5, 9), values[0]);
console.log(Atomics.exchange(values, 0, 2), Atomics.load(values, 0));

