// Cross-runtime: `cause` is installed only when the options object HAS the key
// — `{ cause: undefined }` still creates it — and it lands as a non-enumerable
// own data property on every Error subclass, AggregateError included.
function report(e: any, tag: string): void {
  const has = Object.prototype.hasOwnProperty.call(e, "cause");
  console.log(tag + "-own=" + has);
  console.log(tag + "-in=" + ("cause" in e));
  console.log(tag + "-value=" + String(e.cause));
}

report(new Error("a"), "no-options");
report(new Error("a", {}), "empty-options");
report(new Error("a", { cause: 42 }), "with-cause");
report(new Error("a", { cause: undefined }), "undefined-cause");

const cd: any = Object.getOwnPropertyDescriptor(new Error("a", { cause: 1 }), "cause");
console.log("desc=w" + cd.writable + ",e" + cd.enumerable + ",c" + cd.configurable);
console.log("desc-value=" + cd.value);

// Non-enumerable, so JSON and Object.keys never see it.
const withCause = new Error("outer", { cause: new Error("inner") });
console.log("keys=" + Object.keys(withCause).join(","));
console.log("json=" + JSON.stringify(withCause));
console.log("chain=" + (withCause.cause as any).message);

// An options object that is not an object is ignored without throwing.
report(new Error("a", 5 as any), "number-options");
report(new Error("a", null as any), "null-options");
report(new Error("a", "cause" as any), "string-options");

// A "cause" reached through the prototype chain counts as present.
const protoOpts: any = Object.create({ cause: "inherited" });
report(new Error("a", protoOpts), "inherited-options");

// The getter runs exactly once.
let reads = 0;
const getterOpts: any = {};
Object.defineProperty(getterOpts, "cause", {
  get() {
    reads = reads + 1;
    return "lazy";
  },
  enumerable: true,
  configurable: true,
});
const lazy = new Error("a", getterOpts);
console.log("getter-reads=" + reads);
console.log("getter-value=" + lazy.cause);
console.log("getter-reads-after=" + reads);

// Every subclass takes options in the same second position.
report(new TypeError("t", { cause: "t" }), "typeerror");
report(new RangeError("r", { cause: "r" }), "rangeerror");
report(new SyntaxError("s", { cause: "s" }), "syntaxerror");

// AggregateError takes options THIRD, after errors and message.
const agg = new AggregateError([new Error("x")], "agg", { cause: "aggcause" });
report(agg, "aggregate");
console.log("agg-message=" + agg.message);
console.log("agg-errors-len=" + agg.errors.length);

// A user subclass forwarding options gets the same treatment.
class AppError extends Error {
  code: string;
  constructor(message: string, code: string, options?: any) {
    super(message, options);
    this.code = code;
  }
}
const app = new AppError("app", "E1", { cause: "root" });
report(app, "subclass");
console.log("subclass-code=" + app.code);
console.log("subclass-message=" + app.message);
console.log("subclass-name=" + app.name);
console.log("subclass-keys=" + Object.keys(app).join(","));

const appNoOpts = new AppError("app", "E2");
report(appNoOpts, "subclass-none");

// A cause can be any value, including a primitive or a self-reference.
const selfRef: any = new Error("self");
const wrapper: any = new Error("wrap", { cause: selfRef });
selfRef.cause = wrapper;
console.log("cycle=" + (wrapper.cause.cause === wrapper));
console.log("primitive-cause=" + new Error("p", { cause: null }).cause);
console.log("bool-cause=" + new Error("p", { cause: false }).cause);
