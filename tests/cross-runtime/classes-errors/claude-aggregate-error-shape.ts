// Cross-runtime: AggregateError's argument order (errors, message, options),
// the `errors` own non-enumerable data property, and the iterable-to-array copy
// it performs at construction.
const a = new AggregateError([new TypeError("t"), new RangeError("r")], "two");
console.log("name=" + a.name);
console.log("message=" + a.message);
console.log("errors-len=" + a.errors.length);
console.log("errors-is-array=" + Array.isArray(a.errors));
console.log("errors-0=" + a.errors[0].constructor.name);
console.log("errors-1=" + a.errors[1].constructor.name);
console.log("instanceof-error=" + (a instanceof Error));
console.log("instanceof-agg=" + (a instanceof AggregateError));
console.log("tag=" + Object.prototype.toString.call(a));

const ed: any = Object.getOwnPropertyDescriptor(a, "errors");
console.log("errors-desc=w" + ed.writable + ",e" + ed.enumerable + ",c" + ed.configurable);
console.log("errors-own=" + Object.prototype.hasOwnProperty.call(a, "errors"));
console.log("errors-on-proto=" + Object.prototype.hasOwnProperty.call(AggregateError.prototype, "errors"));
console.log("keys=" + Object.keys(a).join(","));
console.log("json=" + JSON.stringify(a));

// message is optional; omitting it leaves no own message.
const noMsg = new AggregateError([]);
console.log("nomsg-own-message=" + Object.prototype.hasOwnProperty.call(noMsg, "message"));
console.log("nomsg-message=" + JSON.stringify(noMsg.message));
console.log("nomsg-tostring=" + noMsg.toString());
console.log("nomsg-errors-len=" + noMsg.errors.length);

// An explicit undefined message behaves like an omitted one.
const undefMsg = new AggregateError([], undefined);
console.log("undefmsg-own-message=" + Object.prototype.hasOwnProperty.call(undefMsg, "message"));

// Any iterable works, and the result is a fresh plain Array.
const source = new Set(["a", "b", "c"]);
const fromSet: any = new AggregateError(source as any, "set");
console.log("set-len=" + fromSet.errors.length);
console.log("set-values=" + fromSet.errors.join(","));
console.log("set-is-plain=" + (Object.getPrototypeOf(fromSet.errors) === Array.prototype));

const gen = function* () {
  yield 1;
  yield 2;
};
const fromGen: any = new AggregateError(gen(), "gen");
console.log("gen-values=" + fromGen.errors.join(","));

const fromString: any = new AggregateError("hey" as any, "str");
console.log("string-values=" + fromString.errors.join("|"));

// The array is copied, so later mutation of the source is invisible.
const src: any[] = [1, 2];
const copied: any = new AggregateError(src, "copy");
src.push(3);
console.log("copied-len=" + copied.errors.length);
console.log("copied-same=" + (copied.errors === src));

// A non-iterable first argument is a TypeError.
try {
  new AggregateError(5 as any, "bad");
  console.log("non-iterable=no-throw");
} catch (e: any) {
  console.log("non-iterable=" + e.constructor.name);
}
try {
  new AggregateError(undefined as any);
  console.log("undefined-errors=no-throw");
} catch (e: any) {
  console.log("undefined-errors=" + e.constructor.name);
}

// Calling it without new gives the same shape.
const called: any = (AggregateError as any)([new Error("x")], "called");
console.log("called-instanceof=" + (called instanceof AggregateError));
console.log("called-errors=" + called.errors.length);

// Subclassing keeps errors and adds the subclass name resolution.
class Batch extends AggregateError {
  label: string;
  constructor(errs: any[], label: string) {
    super(errs, "batch:" + label);
    this.label = label;
  }
}
const b = new Batch([new Error("q")], "L");
console.log("sub-name=" + b.name);
console.log("sub-message=" + b.message);
console.log("sub-label=" + b.label);
console.log("sub-errors=" + b.errors.length);
console.log("sub-keys=" + Object.keys(b).join(","));
console.log("sub-instanceof=" + (b instanceof AggregateError) + ":" + (b instanceof Error));

// Promise.any over all-rejected produces one, in settle order.
const results: string[] = [];
Promise.any([Promise.reject(new TypeError("p0")), Promise.reject(new RangeError("p1"))]).then(
  () => {
    results.push("resolved");
  },
  (err: any) => {
    results.push("ctor=" + err.constructor.name);
    results.push("len=" + err.errors.length);
    results.push("0=" + err.errors[0].constructor.name);
    results.push("1=" + err.errors[1].constructor.name);
    results.push("is-error=" + (err instanceof Error));
    results.push("errors-is-array=" + Array.isArray(err.errors));
  },
).then(() => {
  console.log("any=" + results.join("|"));
});
console.log("sync-tail=reached");
