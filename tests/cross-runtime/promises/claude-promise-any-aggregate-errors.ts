// Cross-runtime: Promise.any's failure path -- an empty iterable rejects with
// an AggregateError immediately, and `errors` is in INPUT order even when the
// rejections arrive in a different one.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const settled: string[] = [];

// A distinct error CONSTRUCTOR per entry, so `errors` order is readable
// without printing any message.
const kinds: any = { a: RangeError, b: EvalError, c: URIError };
function lateReject(depth: number, tag: string) {
  let p: Promise<any> = Promise.resolve();
  for (let i = 0; i < depth; i++) p = p.then(function () { return undefined; });
  return p.then(function () {
    settled.push(tag);
    throw new kinds[tag]("late");
  });
}

// 1) empty iterable: rejects with an AggregateError carrying no errors
Promise.any([]).then(
  function () { log("empty fulfilled"); },
  function (e: any) {
    log("empty rejected " + e.constructor.name);
    log("empty name=" + e.name);
    log("empty isError=" + (e instanceof Error));
    log("empty isAggregate=" + (e instanceof AggregateError));
    log("empty errorsIsArray=" + Array.isArray(e.errors));
    log("empty errorsLength=" + e.errors.length);
    log("empty ownErrors=" + Object.prototype.hasOwnProperty.call(e, "errors"));
    log("empty errorsEnumerable=" + Object.getOwnPropertyDescriptor(e, "errors").enumerable);
    log("empty tag=" + Object.prototype.toString.call(e));
  }
);

// 2) all reject, out of order: `errors` keeps INPUT order
Promise.any([lateReject(6, "a"), lateReject(2, "b"), lateReject(4, "c")]).then(
  function () { log("allReject fulfilled"); },
  function (e: any) {
    log("allReject " + e.constructor.name + " count=" + e.errors.length);
    log("allReject inputOrder=" + e.errors.map(function (x: any) { return x.constructor.name; }).join(","));
    log("allReject settleOrder=" + settled.join(","));
  }
);

// 3) one late fulfilment beats earlier rejections
Promise.any([
  Promise.reject(new TypeError("x")),
  Promise.reject(new TypeError("y")),
  Promise.resolve("winner")
]).then(
  function (v: any) { log("mixed fulfilled " + v); },
  function (e: any) { log("mixed rejected " + e.constructor.name); }
);

// 4) a non-promise entry wins straight away
Promise.any(["plain", Promise.reject(new TypeError("z"))]).then(
  function (v: any) { log("plainWins " + v); },
  function (e: any) { log("plainWins rejected " + e.constructor.name); }
);

// 5) AggregateError constructed by hand, for the shape comparison
const hand = new AggregateError([1, 2, 3]);
log("handName=" + hand.name);
log("handCount=" + hand.errors.length);
log("handMessageIsString=" + (typeof hand.message === "string"));
log("handProtoIsAggregate=" + (Object.getPrototypeOf(hand) === AggregateError.prototype));
log("handErrorProtoChain=" + (Object.getPrototypeOf(AggregateError.prototype) === Error.prototype));

// 6) the last word, past every chain above
let tail: Promise<any> = Promise.resolve();
for (let i = 0; i < 16; i++) tail = tail.then(function () { return undefined; });
tail.then(function () { console.log("end"); });
