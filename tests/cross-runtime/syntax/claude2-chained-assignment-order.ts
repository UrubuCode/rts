// Cross-runtime: assignment is right-associative and its TARGET REFERENCE is
// resolved before the right-hand side runs. A chain therefore resolves every
// target left to right, then computes the value once and stores it in each.

const trace: string[] = [];
function note<T>(label: string, v: T): T {
  trace.push(label);
  return v;
}

// 1) A chain assigns one value to every target and evaluates to that value.
let a = 0, b = 0, c = 0;
const result = (a = b = c = 7);
console.log("chain_values=" + a + "," + b + "," + c + "|result=" + result);

// 2) The right-hand side runs exactly once.
let calls = 0;
function source(): number {
  calls += 1;
  return 42;
}
let p = 0, q = 0;
p = q = source();
console.log("rhs_once=" + p + "," + q + "|calls=" + calls);

// 3) The targets' references are resolved LEFT to right, before the value.
const holder: any = { first: null, second: null };
trace.length = 0;
function objFor(label: string): any {
  trace.push("ref:" + label);
  return holder;
}
objFor("outer").first = objFor("inner").second = note("value", "V");
console.log("ref_order=" + trace.join(","));
console.log("ref_result=" + holder.first + "/" + holder.second);

// 4) The classic consequence: `o.x = o = other` writes into the OLD object,
//    because `o.x`'s reference was taken before `o` was reassigned.
let o: any = { name: "original" };
const original = o;
const replacement: any = { name: "replacement" };
o.x = o = replacement;
console.log("old_object_got_x=" + (original.x === replacement));
console.log("new_object_has_x=" + ("x" in o));
console.log("o_is_replacement=" + (o === replacement));

// 5) A computed key is evaluated with the reference, before the value.
trace.length = 0;
const target: any = {};
target[note("key", "k")] = note("val", 1);
console.log("computed_key_order=" + trace.join(","));

// 6) In a chain of computed keys, all keys come first, then the value.
trace.length = 0;
const t1: any = {}, t2: any = {};
t1[note("k1", "a")] = t2[note("k2", "b")] = note("v", 9);
console.log("chain_key_order=" + trace.join(","));
console.log("chain_key_result=" + t1.a + "/" + t2.b);

// 7) The expression's value is what was assigned, not what a setter stored.
const setterHolder: any = {
  _v: "",
  set slot(v: string) { this._v = "stored:" + v; },
  get slot(): string { return this._v; },
};
const assigned = (setterHolder.slot = "raw");
console.log("assign_value_vs_getter=" + assigned + "|" + setterHolder.slot);

// 8) A chain through a setter still passes the ORIGINAL value onwards.
let tail = "";
const chainSetter: any = { set s(v: string) { tail = "via-setter:" + v; } };
let plain = "";
plain = chainSetter.s = "shared";
console.log("chain_through_setter=" + plain + "|" + tail);

// 9) Self-assignment with a postfix increment: the old value wins.
let i = 0;
i = i++;
console.log("self_postfix=" + i);
let j = 0;
j = ++j;
console.log("self_prefix=" + j);

// 10) Index and value ordering when both mutate the same counter.
const arr: any[] = [];
let n = 0;
arr[n++] = n;
console.log("index_before_value=" + JSON.stringify(arr) + "|n=" + n);

// 11) Swapping through an array pattern needs no temporary.
let x = "left", y = "right";
[x, y] = [y, x];
console.log("swap=" + x + "," + y);

// 12) A rotation of three, all read before any is written.
let r1 = 1, r2 = 2, r3 = 3;
[r1, r2, r3] = [r3, r1, r2];
console.log("rotate=" + r1 + "," + r2 + "," + r3);

// 13) Destructuring assignment to member expressions resolves the objects in
//     source order, before the iterable is consumed.
trace.length = 0;
const sink: any = { a: null, b: null };
function sinkFor(label: string): any {
  trace.push("sink:" + label);
  return sink;
}
function values(): any {
  trace.push("iterable");
  return ["A", "B"];
}
[sinkFor("one").a, sinkFor("two").b] = values();
console.log("destructure_member_order=" + trace.join(","));
console.log("destructure_member_result=" + sink.a + sink.b);

// 14) An object destructuring assignment needs parentheses as a statement, and
//     its value is the SOURCE object.
let da = 0, db = 0;
const src = { da: 1, db: 2 };
const daResult = ({ da, db } = src);
console.log("obj_destructure=" + da + "," + db + "|result_is_src=" + (daResult === src));

// 15) Chained assignment mixed with a compound operator: the compound reads its
//     target once and the chain still returns the final value.
let base = 10;
let mirror = 0;
const compound = (mirror = base += 5);
console.log("compound_chain=" + base + "," + mirror + "|value=" + compound);

// 16) Assignment inside a condition: the value is what the test sees.
const conditionSeen: string[] = [];
let cursor = 3;
let picked = 0;
while ((picked = cursor -= 1) > 0) conditionSeen.push(String(picked));
console.log("assign_in_condition=" + conditionSeen.join(",") + "|cursor=" + cursor);

// 17) Assignment as an argument passes the assigned value along.
function takes(v: string): string { return "got:" + v; }
let passed = "";
console.log("assign_as_arg=" + takes((passed = "inline")) + "|" + passed);

// 18) A chain whose middle target is a getter-only property: the write is
//     probed rather than performed, so the value still reaches the last target.
const readOnly: any = {};
Object.defineProperty(readOnly, "ro", { get() { return "fixed"; }, configurable: true });
const accepted = Reflect.set(readOnly, "ro", "attempt");
console.log("readonly_accepted=" + accepted + "|value=" + readOnly.ro);

// 19) Assignment to a frozen object's property, probed the same way.
const frozen = Object.freeze({ f: 1 });
console.log("frozen_accepted=" + Reflect.set(frozen, "f", 2) + "|value=" + (frozen as any).f);

// 20) The chain is right-associative: parenthesising the left pair is a
//     different program, and it is not a valid target — so only the natural
//     grouping assigns to both.
let z1 = 0, z2 = 0;
z1 = z2 = 5;
console.log("right_assoc=" + z1 + "," + z2);

// 21) Nested chains inside an object literal value position.
let outerVal = 0;
const literal: any = { field: (outerVal = 11) };
console.log("in_literal=" + literal.field + "|" + outerVal);
