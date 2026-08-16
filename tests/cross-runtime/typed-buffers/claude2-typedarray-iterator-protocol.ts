// Cross-runtime: a typed array iterates through the SAME %ArrayIteratorPrototype%
// an Array uses — values() is Symbol.iterator, the iterator reads the live
// elements as it goes, and it refuses to run once the buffer is detached.

const ta = new Uint8Array([1, 2, 3]);

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

console.log("iterator_is_values=" + (ta[Symbol.iterator] === (ta as any).values));
console.log("shared_with_array=" + ((ta as any).values === (Array.prototype as any).values));
const arrayIterProto = Object.getPrototypeOf(Object.getPrototypeOf([].values()));
const typedIterProto = Object.getPrototypeOf(Object.getPrototypeOf(ta.values()));
console.log("iterator_proto_shared=" + (Object.getPrototypeOf(ta.values()) === Object.getPrototypeOf([].values())));
console.log("iterator_intrinsic_shared=" + (arrayIterProto === typedIterProto));
console.log("iterator_tag=" + Object.prototype.toString.call(ta.values()));
console.log("iterator_self=" + (function (): string {
  const it: any = ta.values();
  return String(it[Symbol.iterator]() === it);
})());

console.log("values=" + JSON.stringify(Array.from(ta.values())));
console.log("keys=" + JSON.stringify(Array.from(ta.keys())));
console.log("entries=" + JSON.stringify(Array.from(ta.entries())));
console.log("entries_pair_is_array=" + Array.isArray(Array.from(ta.entries())[0]));
console.log("empty_values=" + JSON.stringify(Array.from(new Uint8Array(0).values())));

console.log("step_shape=" + (function (): string {
  const it = ta.values();
  const first: any = it.next();
  return JSON.stringify(first) + "/" + Object.keys(first).sort().join(",");
})());
console.log("after_done=" + (function (): string {
  const it: any = new Uint8Array([1]).values();
  it.next();
  const done = it.next();
  return done.done + "/" + String(done.value) + "/" + JSON.stringify(it.next());
})());
console.log("next_length=" + (ta.values() as any).next.length);
console.log("has_return=" + typeof (ta.values() as any).return + " has_throw=" + typeof (ta.values() as any).throw);

// The iterator reads elements lazily, so a write during the walk is seen.
console.log("live_reads=" + (function (): string {
  const a = new Uint8Array([1, 2, 3]);
  const out: number[] = [];
  for (const v of a) {
    out.push(v);
    a[2] = 9;
  }
  return out.join(",");
})());
console.log("for_of_bigint=" + Array.from(new BigInt64Array([1n, 2n])).join(","));
console.log("destructure=" + (function (): string {
  const [a, , c] = new Uint8Array([4, 5, 6]);
  return a + "/" + c;
})());
console.log("rest=" + (function (): string {
  const [head, ...tail] = new Uint8Array([7, 8, 9]);
  return head + "/" + JSON.stringify(tail) + "/" + Array.isArray(tail);
})());

// Array.from and the spread go through the iterator; Array.from also accepts the
// array-like path, which is how a receiver without Symbol.iterator still works.
console.log("array_from_kind=" + Array.from(ta).constructor.name + " spread_kind=" + [...ta].constructor.name);
console.log("array_from_mapped=" + Array.from(ta, function (x) { return x * 10; }).join(","));
console.log("from_without_iterator=" + t(function () {
  const a: any = new Uint8Array([1, 2]);
  const saved = a[Symbol.iterator];
  a[Symbol.iterator] = undefined;
  const out = Array.from(a).join(",");
  a[Symbol.iterator] = saved;
  return out;
}));
console.log("spread_without_iterator=" + t(function () {
  const a: any = new Uint8Array([1, 2]);
  a[Symbol.iterator] = undefined;
  return [...a].join(",");
}));

// A custom Symbol.iterator on the instance shadows the inherited one.
console.log("shadowed_iterator=" + t(function () {
  const a: any = new Uint8Array([1, 2]);
  a[Symbol.iterator] = function* () { yield 42; };
  return [...a].join(",") + "/" + Array.from(a).join(",");
}));

// Detaching mid-walk stops the iterator with a TypeError.
console.log("detached_next=" + t(function () {
  const buf = new ArrayBuffer(2);
  const a = new Uint8Array(buf);
  const it = a[Symbol.iterator]();
  it.next();
  buf.transfer();
  return JSON.stringify(it.next());
}));
console.log("detached_from_start=" + t(function () {
  const buf = new ArrayBuffer(2);
  const a = new Uint8Array(buf);
  buf.transfer();
  return JSON.stringify(Array.from(a));
}));
console.log("map_key_identity=" + (function (): string {
  const key = new Uint8Array([1]);
  const m = new Map([[key, "v"]]);
  return String(m.get(key)) + "/" + String(m.get(new Uint8Array([1]))) + "/" + m.size;
})());
