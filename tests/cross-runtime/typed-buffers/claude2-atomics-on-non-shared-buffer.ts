// Cross-runtime: Atomics works on an ORDINARY ArrayBuffer, not only a shared one.
// Every operation answers the value it read BEFORE the write, the index is
// bounds-checked, and a non-integer element kind is refused with a TypeError.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

const i32 = new Int32Array(new ArrayBuffer(16));

console.log("store_returns_value=" + Atomics.store(i32, 0, 5) + " load=" + Atomics.load(i32, 0));
console.log("add_returns_old=" + Atomics.add(i32, 0, 3) + " now=" + i32[0]);
console.log("sub_returns_old=" + Atomics.sub(i32, 0, 1) + " now=" + i32[0]);
console.log("and_returns_old=" + Atomics.and(i32, 0, 6) + " now=" + i32[0]);
console.log("or_returns_old=" + Atomics.or(i32, 0, 1) + " now=" + i32[0]);
console.log("xor_returns_old=" + Atomics.xor(i32, 0, 3) + " now=" + i32[0]);
console.log("exchange_returns_old=" + Atomics.exchange(i32, 0, 9) + " now=" + i32[0]);
console.log("cas_match=" + Atomics.compareExchange(i32, 0, 9, 4) + " now=" + i32[0]);
console.log("cas_mismatch=" + Atomics.compareExchange(i32, 0, 99, 7) + " unchanged=" + i32[0]);

// The value goes through the element kind's own conversion, and store answers
// the CONVERTED-BEFORE-truncation value rather than what landed in the slot.
console.log("store_fraction=" + Atomics.store(i32, 1, 2.9 as any) + " stored=" + i32[1]);
console.log("store_string=" + Atomics.store(i32, 1, "7" as any) + " stored=" + i32[1]);
console.log("store_nan=" + Atomics.store(i32, 1, NaN as any) + " stored=" + i32[1]);
console.log("store_undefined=" + t(function () { return Atomics.store(i32, 1, undefined as any) + "/" + i32[1]; }));
console.log("wrap_int8=" + t(function () {
  const a = new Int8Array(2);
  return Atomics.store(a, 0, 200) + "/" + a[0] + "/" + Atomics.add(a, 0, 100) + "/" + a[0];
}));
console.log("wrap_uint32=" + t(function () {
  const a = new Uint32Array(1);
  Atomics.store(a, 0, -1);
  return a[0] + "/" + Atomics.load(a, 0);
}));

// The index runs ToIndex against the array's length.
console.log("index_string=" + t(function () { Atomics.store(i32, "2" as any, 6); return String(i32[2]); }));
console.log("index_fraction=" + t(function () { return String(Atomics.load(i32, 1.5 as any)); }));
console.log("index_negative=" + t(function () { return Atomics.load(i32, -1); }));
console.log("index_past_end=" + t(function () { return Atomics.load(i32, 4); }));
console.log("index_undefined=" + t(function () { return String(Atomics.load(i32, undefined as any)); }));
console.log("index_nan=" + t(function () { return String(Atomics.load(i32, NaN as any)); }));

// Which element kinds Atomics accepts at all.
const kinds = ["Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array", "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array"];
for (const name of kinds) {
  console.log("kind_" + name + "=" + t(function () {
    const ctor: any = (globalThis as any)[name];
    return String(Atomics.load(new ctor(2), 0));
  }));
}
console.log("plain_array=" + t(function () { return Atomics.load([1, 2] as any, 0); }));
console.log("dataview=" + t(function () { return Atomics.load(new DataView(new ArrayBuffer(8)) as any, 0); }));
console.log("detached=" + t(function () {
  const b = new ArrayBuffer(8);
  const a = new Int32Array(b);
  b.transfer();
  return Atomics.load(a, 0);
}));

// The bigint kinds take bigint operands, and only bigint operands.
console.log("bigint_ops=" + t(function () {
  const b = new BigInt64Array(2);
  Atomics.store(b, 0, 5n);
  return String(Atomics.add(b, 0, 2n)) + "/" + b[0] + "/" + String(Atomics.compareExchange(b, 0, 7n, 1n)) + "/" + b[0];
}));
console.log("bigint_number_operand=" + t(function () { return Atomics.store(new BigInt64Array(1), 0, 5 as any); }));
console.log("number_bigint_operand=" + t(function () { return Atomics.store(new Int32Array(1), 0, 5n as any); }));

// wait needs a SHARED buffer; notify does not, and simply reports zero wakes.
console.log("wait_non_shared=" + t(function () { return (Atomics as any).wait(i32, 0, 0, 0); }));
console.log("notify_non_shared=" + t(function () { return Atomics.notify(i32, 0, 0); }));
console.log("notify_default_count=" + t(function () { return Atomics.notify(i32, 0); }));
console.log("isLockFree=" + [1, 2, 4, 8].map(function (n) { return typeof Atomics.isLockFree(n); }).join(",") + " four=" + Atomics.isLockFree(4));
console.log("namespace=" + Object.prototype.toString.call(Atomics) + " callable=" + t(function () { return (Atomics as any)(); }));
console.log("members=" + ["add", "and", "compareExchange", "exchange", "isLockFree", "load", "notify", "or", "store", "sub", "wait", "waitAsync", "xor"].map(function (k) { return typeof (Atomics as any)[k]; }).join(","));
