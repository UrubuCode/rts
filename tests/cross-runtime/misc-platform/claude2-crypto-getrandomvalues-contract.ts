// Cross-runtime: the DETERMINISTIC half of crypto.getRandomValues — it fills the
// SAME object it is handed, only inside that view's window, only for the integer
// element kinds, and every byte it writes is in range for that kind.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name + "/" + e.name;
  }
};

const c: any = (globalThis as any).crypto;

console.log("global=" + typeof c + " tag=" + Object.prototype.toString.call(c));
console.log("subtle=" + typeof c.subtle + " tag=" + Object.prototype.toString.call(c.subtle));
console.log("members=" + ["getRandomValues", "randomUUID", "subtle"].map(function (k) { return k + ":" + typeof c[k]; }).join(" "));
console.log("brand=" + c.constructor.name + " constructible=" + t(function () { return new (c.constructor)(); }));
console.log("methods_not_constructible=" + t(function () { return new (c.getRandomValues as any)(new Uint8Array(1)); }));

// It answers the very object it was given, not a copy.
console.log("returns_same_object=" + (function (): string {
  const a = new Uint8Array(8);
  return String(c.getRandomValues(a) === a) + "/" + a.length;
})());
console.log("empty_array=" + (function (): string {
  const a = new Uint8Array(0);
  return String(c.getRandomValues(a) === a) + "/" + a.length;
})());
console.log("max_size=" + (function (): string {
  const a = new Uint8Array(65536);
  return String(c.getRandomValues(a).length);
})());

// The integer kinds are accepted; the values land inside each kind's range.
const integerKinds = ["Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array"];
for (const name of integerKinds) {
  console.log("kind_" + name + "=" + t(function () {
    const ctor: any = (globalThis as any)[name];
    const a = ctor.from(new ctor(64));
    c.getRandomValues(a);
    const values: number[] = Array.from(a);
    const min = Math.min.apply(null, values);
    const max = Math.max.apply(null, values);
    const bits = ctor.BYTES_PER_ELEMENT * 8;
    const signed = name.charAt(0) === "I";
    const lo = signed ? -Math.pow(2, bits - 1) : 0;
    const hi = signed ? Math.pow(2, bits - 1) - 1 : Math.pow(2, bits) - 1;
    return "inRange:" + (min >= lo && max <= hi) + " integers:" + values.every(function (v) { return Number.isInteger(v); });
  }));
}
console.log("bigint_kinds=" + ["BigInt64Array", "BigUint64Array"].map(function (name) {
  return name + ":" + t(function () {
    const ctor: any = (globalThis as any)[name];
    const a = new ctor(4);
    c.getRandomValues(a);
    return String(Array.from(a).every(function (v: any) { return typeof v === "bigint"; }));
  });
}).join(" "));
console.log("plain_array=" + t(function () { return c.getRandomValues([1, 2, 3]); }));
console.log("string=" + t(function () { return c.getRandomValues("abc"); }));
console.log("object=" + t(function () { return c.getRandomValues({ length: 4 }); }));

// It writes only inside the view's own window.
console.log("respects_window=" + (function (): string {
  const backing = new Uint8Array(16);
  const window = backing.subarray(4, 8);
  c.getRandomValues(window);
  const before = Array.from(backing.subarray(0, 4)).join(",");
  const after = Array.from(backing.subarray(8)).join(",");
  return "before:" + (before === "0,0,0,0") + " after:" + (after === "0,0,0,0,0,0,0,0") + " len:" + window.length;
})());
console.log("detached=" + t(function () {
  const buf = new ArrayBuffer(8);
  const a = new Uint8Array(buf);
  buf.transfer();
  return c.getRandomValues(a).length;
}));

// randomUUID answers a version-4 variant-1 UUID, and never the same one twice.
const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
console.log("uuid_shape=" + (function (): string {
  const u: string = c.randomUUID();
  return u.length + "/" + uuidPattern.test(u) + "/" + (typeof u) + "/" + u.charAt(14);
})());
console.log("uuid_variant=" + (function (): string {
  const u: string = c.randomUUID();
  return String("89ab".indexOf(u.charAt(19)) >= 0);
})());
console.log("uuid_hyphens=" + (function (): string {
  const u: string = c.randomUUID();
  return u.split("-").map(function (p) { return String(p.length); }).join(",");
})());
console.log("uuid_lowercase=" + (function (): string {
  const u: string = c.randomUUID();
  return String(u === u.toLowerCase());
})());
console.log("uuid_unique=" + (function (): string {
  const seen = new Set<string>();
  for (let i = 0; i < 16; i++) seen.add(c.randomUUID());
  return String(seen.size);
})());
console.log("uuid_all_valid=" + (function (): string {
  for (let i = 0; i < 8; i++) {
    if (!uuidPattern.test(c.randomUUID())) return "false";
  }
  return "true";
})());
