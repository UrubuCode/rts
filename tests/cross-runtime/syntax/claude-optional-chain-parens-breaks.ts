// Cross-runtime: an optional chain short-circuits to the END of the chain, and
// parentheses END the chain. `(a?.b).c` therefore throws where `a?.b.c` does not.

const nil: any = null;
const undef: any = undefined;
const obj: any = { b: { c: { d: 7 } }, list: [10, 20], fn(): string { return "called"; } };

// The whole chain short-circuits, however long.
console.log("deep_chain=" + nil?.b.c.d);
console.log("deep_chain_type=" + typeof nil?.b.c.d);
console.log("undef_chain=" + undef?.b.c.d);
console.log("index_chain=" + nil?.b[0].c);
console.log("call_chain=" + nil?.b.c());
console.log("mixed_chain=" + nil?.b[0]().c);

// Parentheses stop the chain: the outer access runs against `undefined`.
try {
  const v = (nil?.b).c;
  console.log("parens_threw=false:" + v);
} catch (e) {
  console.log("parens_threw=true:" + (e as any).constructor.name);
}
try {
  const v = (nil?.b)[0];
  console.log("parens_index_threw=false:" + v);
} catch (e) {
  console.log("parens_index_threw=true:" + (e as any).constructor.name);
}

// Without the parentheses, the same shapes are silent.
console.log("no_parens_prop=" + nil?.b.c);
console.log("no_parens_call=" + nil?.b());
console.log("no_parens_index=" + nil?.b[0]);

// Nothing after the `?.` is EVALUATED when it short-circuits.
const calls: string[] = [];
function note(name: string): any { calls.push(name); return 0; }
nil?.b[note("index")];
nil?.b(note("arg"));
nil?.[note("computed")];
nil?.b.c[note("deep_index")].d(note("deep_arg"));
console.log("no_side_effects=" + calls.length + ":[" + calls.join(",") + "]");

// On a present value the same expressions DO evaluate.
obj?.b[note("index_live")];
obj?.fn(note("arg_live"));
console.log("side_effects_when_present=" + calls.join(","));

// `delete` over a short-circuiting chain is true and evaluates nothing.
calls.length = 0;
console.log("delete_short=" + delete nil?.b.c);
console.log("delete_short_index=" + delete nil?.b[note("del")]);
console.log("delete_no_eval=" + calls.length);

// `delete` on a live chain actually removes.
const target: any = { keep: 1, drop: 2 };
console.log("delete_live=" + delete target?.drop);
console.log("after_delete=" + Object.keys(target).join(","));

// The short-circuit value is `undefined`, never `null`, even from `null`.
console.log("is_undefined=" + (nil?.b === undefined));
console.log("not_null=" + (nil?.b === null));

// It combines with `??` and arithmetic the ordinary way.
console.log("with_nullish=" + (nil?.b ?? "fallback"));
console.log("with_arith=" + (nil?.b + 1));
console.log("with_template=" + `${nil?.b}`);

// A short-circuit inside a longer expression stops only its own chain.
console.log("sibling_chain=" + (nil?.b) + "/" + obj?.b.c.d);

// `?.` guards only the value immediately to its LEFT.
try {
  const v = obj.missing?.deep.value;
  console.log("guard_left_ok=" + v);
} catch (e) {
  console.log("guard_left_threw=" + (e as any).constructor.name);
}
try {
  const v = obj.missing.deep?.value;
  console.log("guard_too_late=false:" + v);
} catch (e) {
  console.log("guard_too_late=true:" + (e as any).constructor.name);
}

// A parenthesised chain that does NOT short-circuit is unaffected.
console.log("parens_live=" + (obj?.b).c.d);

// Optional call on a missing method vs on a non-function.
console.log("optional_missing_method=" + obj.nothere?.());
try {
  const v = (obj as any).b?.();
  console.log("optional_non_function=false:" + v);
} catch (e) {
  console.log("optional_non_function=true:" + (e as any).constructor.name);
}

// The receiver of an optional call is still bound correctly.
const bound: any = {
  tag: "self",
  read(): string { return this.tag; },
};
console.log("this_binding=" + bound?.read());
console.log("this_binding_computed=" + bound?.["read"]());

// A short-circuit inside a nested chain does not cancel the outer expression.
const outerCalls: string[] = [];
function outer(v: any): string { outerCalls.push(String(v)); return "outer"; }
console.log("nested_result=" + outer(nil?.b.c));
console.log("outer_ran=" + outerCalls.join(","));

// Assignment through an optional chain is not allowed as a target, but the
// value side works, including when the chain feeds a compound expression.
let acc = 5;
acc += nil?.b ?? 3;
console.log("compound_from_chain=" + acc);
