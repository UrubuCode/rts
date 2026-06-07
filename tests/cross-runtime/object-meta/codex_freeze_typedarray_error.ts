// Cross-runtime: freezing a non-empty typed array throws.
const empty = new Uint8Array(0);
const full = new Uint8Array([1]);
console.log(Object.isFrozen(Object.freeze(empty)));
try {
  Object.freeze(full);
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log(full[0]);
