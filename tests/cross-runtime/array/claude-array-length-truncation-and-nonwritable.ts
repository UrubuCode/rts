// ONE thing: length as a data property — truncation deletes elements, a
// non-configurable element blocks it partway, and a bad value throws RangeError
// rather than coercing.
const a = [0, 1, 2, 3, 4];
a.length = 2;
console.log("trunc=" + JSON.stringify(a) + " len=" + a.length);
console.log("idx2=" + String(a[2]) + " has2=" + (2 in a));

a.length = 5;
console.log("grow=" + a.length + " holes=" + [0, 1, 2, 3, 4].map((i) => (i in a ? "y" : "n")).join(""));

const b = [1, 2, 3];
b.length = "1" as any;
console.log("strLen=" + b.length + " " + JSON.stringify(b));

for (const bad of [-1, 1.5, NaN, 4294967296, Infinity]) {
  const c = [1, 2, 3];
  try { c.length = bad as any; console.log("ok " + String(bad) + " -> " + c.length); }
  catch (e: any) { console.log("throw " + String(bad) + " -> " + e.constructor.name); }
}

// A non-configurable element stops truncation at ITS index, and length reports
// how far it got. Probed with Reflect.set, whose boolean result is the same in
// strict and sloppy code — a bare assignment would only THROW in strict mode.
const d = [0, 1, 2, 3, 4];
Object.defineProperty(d, 2, { value: 22, configurable: false });
console.log("blockedSet=" + Reflect.set(d, "length", 0));
console.log("blockedLen=" + d.length + " kept=" + d.map(String).join(","));

const desc: any = Object.getOwnPropertyDescriptor([1], "length");
console.log("desc=" + [desc.writable, desc.enumerable, desc.configurable].join(","));

// Freezing makes length non-writable, so the write is refused outright.
const f = Object.freeze([1, 2]);
console.log("frozenSet=" + Reflect.set(f, "length", 0) + " len=" + f.length);
console.log("frozenDesc=" + Object.getOwnPropertyDescriptor(f, "length")!.writable);
