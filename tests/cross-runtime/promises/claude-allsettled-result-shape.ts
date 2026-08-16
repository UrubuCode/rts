// Cross-runtime: the exact SHAPE of a Promise.allSettled result entry -- own
// key order, absence of the opposite key, prototype, and that the array is a
// plain Array in input order regardless of settle order.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const order: string[] = [];

// Three entries that settle in the reverse of their input order, driven by
// microtask depth only (no timers).
function late(depth: number, value: any, reject: boolean) {
  let p: Promise<any> = Promise.resolve();
  for (let i = 0; i < depth; i++) p = p.then(function () { return undefined; });
  return p.then(function () {
    order.push((reject ? "reject:" : "resolve:") + value);
    if (reject) throw new RangeError(String(value));
    return value;
  });
}

const entries = [
  late(6, "a", false),
  late(4, "b", true),
  late(2, "c", false),
  "notAPromise",
  Promise.reject(new TypeError("d"))
];

Promise.allSettled(entries as any).then(function (rs: any[]) {
  log("isArray=" + Array.isArray(rs));
  log("length=" + rs.length);
  log("settleOrder=" + order.join("|"));

  for (let i = 0; i < rs.length; i++) {
    const r: any = rs[i];
    log("[" + i + "] keys=" + Object.keys(r).join(","));
    log("[" + i + "] status=" + r.status);
    log("[" + i + "] hasValue=" + ("value" in r) + " hasReason=" + ("reason" in r));
    const payload = r.status === "fulfilled" ? String(r.value) : r.reason.constructor.name;
    log("[" + i + "] payload=" + payload);
  }

  const first: any = rs[0];
  log("protoIsObject=" + (Object.getPrototypeOf(first) === Object.prototype));
  log("tag=" + Object.prototype.toString.call(first));
  log("descWritable=" + Object.getOwnPropertyDescriptor(first, "status").writable);
  log("descEnumerable=" + Object.getOwnPropertyDescriptor(first, "status").enumerable);
  log("descConfigurable=" + Object.getOwnPropertyDescriptor(first, "status").configurable);
  log("resultsAreDistinct=" + (rs[0] !== rs[2]));
  log("arrayProto=" + (Object.getPrototypeOf(rs) === Array.prototype));
  console.log("end");
});
