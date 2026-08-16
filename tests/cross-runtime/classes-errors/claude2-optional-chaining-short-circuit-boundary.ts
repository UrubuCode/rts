// Cross-runtime: exactly where `?.` stops a TypeError and where it does not.
// The short circuit swallows the WHOLE remaining chain when the tested value is
// null or undefined, but a `?.` earlier in a chain does nothing for a plain `.`
// that follows it once the chain has resumed.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

const missing: any = undefined;
const nulled: any = null;
const present: any = { inner: { leaf: "leaf-value" }, fn: () => "called", notFn: 7 };
const shallow: any = { inner: undefined };

// A null or undefined base short-circuits everything to its right.
console.log("undef-one=" + probe(() => missing?.a));
console.log("undef-deep=" + probe(() => missing?.a.b.c));
console.log("undef-call=" + probe(() => missing?.a()));
console.log("undef-index=" + probe(() => missing?.[0][1]));
console.log("null-deep=" + probe(() => nulled?.a.b.c));
console.log("null-call=" + probe(() => nulled?.fn()));

// Once the chain resumes on a real value, a plain `.` on an undefined result
// throws exactly as it always did.
console.log("resumed-plain=" + probe(() => shallow?.inner.leaf));
console.log("resumed-optional=" + probe(() => shallow?.inner?.leaf));
console.log("present-plain=" + probe(() => present?.inner.leaf));
console.log("present-missing=" + probe(() => present?.nothing.leaf));
console.log("present-missing-optional=" + probe(() => present?.nothing?.leaf));

// The short circuit does NOT cross parentheses: `(a?.b).c` re-enters an
// ordinary member access on undefined.
console.log("parens-break=" + probe(() => (missing?.a).b));
console.log("parens-keep=" + probe(() => missing?.a.b));

// Optional CALL: `?.()` tests the callee, not the object.
console.log("optcall-present=" + probe(() => present.fn?.()));
console.log("optcall-absent=" + probe(() => present.nothing?.()));
console.log("optcall-not-function=" + probe(() => present.notFn?.()));
console.log("call-absent-plain=" + probe(() => present.nothing()));

// Optional index access.
console.log("optindex-absent=" + probe(() => missing?.[compute()]));
console.log("optindex-present=" + probe(() => present?.["inner"]["leaf"]));

// Arguments and the index expression are NOT evaluated when the chain is cut.
const evaluated: string[] = [];
function compute(): string {
  evaluated.push("index");
  return "inner";
}
function arg(): number {
  evaluated.push("arg");
  return 1;
}
console.log("skip-eval=" + probe(() => missing?.method(arg())));
console.log("evaluated-after-skip=" + JSON.stringify(evaluated.join(",")));
console.log("run-eval=" + probe(() => present.fn?.(arg())));
console.log("evaluated-after-run=" + evaluated.join(","));

// delete with an optional chain short-circuits to true.
console.log("optional-delete-missing=" + probe(() => delete missing?.a.b));
console.log("optional-delete-present=" + probe(() => delete present?.notFn));
console.log("notfn-after-delete=" + String(present.notFn));

// Optional chaining on a primitive is fine — only null and undefined cut.
console.log("on-number=" + probe(() => (0 as any)?.toFixed(1)));
console.log("on-empty-string=" + probe(() => ("" as any)?.length));
console.log("on-false=" + probe(() => (false as any)?.toString()));
console.log("on-nan=" + probe(() => (NaN as any)?.toString()));

// `??` and `?.` compose, and `??` never sees the error a plain `.` would raise.
console.log("nullish-default=" + probe(() => missing?.a ?? "fallback"));
console.log("nullish-zero=" + probe(() => (present.zero ?? "fallback")));
console.log("nullish-after-throw=" + probe(() => (missing.a ?? "fallback")));

// A method reached through an optional chain keeps its receiver.
const receiver: any = {
  tag: "self",
  read(): string {
    return "read:" + this.tag;
  },
};
console.log("receiver-kept=" + probe(() => receiver?.read()));
console.log("receiver-kept-optional=" + probe(() => receiver.read?.()));
const detached = receiver.read;
console.log("receiver-rebound=" + probe(() => detached.call({ tag: "other" })));
console.log("receiver-empty=" + probe(() => detached.call({})));

// The same boundary applies through a class instance with a getter that
// returns undefined.
class Holder {
  get maybe(): any {
    return undefined;
  }
  get always(): any {
    return { deep: "deep-value" };
  }
}
const h = new Holder();
console.log("class-optional=" + probe(() => h.maybe?.deep));
console.log("class-plain=" + probe(() => h.maybe.deep));
console.log("class-always=" + probe(() => h.always?.deep));
console.log("class-always-plain=" + probe(() => h.always.deep));
