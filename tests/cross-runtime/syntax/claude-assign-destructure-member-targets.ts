// Cross-runtime: destructuring ASSIGNMENT whose targets are member expressions.
// Each target's object and key are evaluated when that element is reached, so
// the trace interleaves target evaluation, source reads and setter calls.

const trace: string[] = [];
function note<T>(label: string, value: T): T { trace.push(label); return value; }

// Array pattern with member targets.
const box: any = {};
const holder: any = { arr: [0, 0, 0] };
let idx = 0;
function nextKey(): number { trace.push("key" + idx); return idx++; }

trace.length = 0;
[box.a, holder.arr[nextKey()], box.b] = [1, 2, 3];
console.log("array_targets=" + trace.join(","));
console.log("array_result=" + box.a + "," + holder.arr.join("") + "," + box.b);

// The object part of a member target is evaluated too, in order.
trace.length = 0;
const t1: any = {};
const t2: any = {};
[note("obj1", t1).x, note("obj2", t2).y] = [10, 20];
console.log("object_eval=" + trace.join(","));
console.log("object_result=" + t1.x + "," + t2.y);

// Object pattern with member targets and a computed source key.
trace.length = 0;
const dst: any = { inner: {} };
function srcKey(): string { trace.push("srckey"); return "p"; }
({ [srcKey()]: dst.taken, q: dst.inner.deep } = { p: "P", q: "Q" });
console.log("object_pattern_eval=" + trace.join(","));
console.log("object_pattern_result=" + dst.taken + "," + dst.inner.deep);

// Defaults only run when the source value is `undefined`.
trace.length = 0;
const withDefaults: any = {};
function dflt(tag: string, v: any): any { trace.push("dflt:" + tag); return v; }
[withDefaults.present = dflt("a", 99), withDefaults.missing = dflt("b", 42)] = [7];
console.log("defaults_eval=" + trace.join(","));
console.log("defaults_result=" + withDefaults.present + "," + withDefaults.missing);

// `null` in the source does NOT trigger the default.
const nullSource: any = {};
[nullSource.v = "unused"] = [null];
console.log("null_no_default=" + String(nullSource.v));

// Setters on the target run in element order and see the assigned value.
trace.length = 0;
const setters: any = {
  _x: 0,
  _y: 0,
  set x(v: number) { trace.push("set_x=" + v); this._x = v; },
  set y(v: number) { trace.push("set_y=" + v); this._y = v; },
};
[setters.x, setters.y] = [11, 22];
console.log("setter_order=" + trace.join(","));
console.log("setter_state=" + setters._x + "/" + setters._y);

// Getters on the SOURCE run interleaved with the targets.
trace.length = 0;
const source: any = {
  get first() { trace.push("get_first"); return 1; },
  get second() { trace.push("get_second"); return 2; },
};
const into: any = {};
({ first: into.a, second: into.b } = source);
console.log("getter_order=" + trace.join(","));
console.log("getter_result=" + into.a + into.b);

// Nested patterns descend before assigning.
trace.length = 0;
const nest: any = { out: {} };
[[nest.out.p], { q: nest.out.q }] = [[5], { q: 6 }];
console.log("nested_result=" + nest.out.p + "," + nest.out.q);

// A rest element writes an Array to a member target.
const restTarget: any = {};
[restTarget.head, ...restTarget.tail] = [1, 2, 3, 4];
console.log("rest_head=" + restTarget.head);
console.log("rest_tail=" + restTarget.tail.join("-"));
console.log("rest_is_array=" + Array.isArray(restTarget.tail));

// An object rest onto a member target.
const objRest: any = {};
({ keep: objRest.kept, ...objRest.others } = { keep: 1, a: 2, b: 3 });
console.log("obj_rest_kept=" + objRest.kept);
console.log("obj_rest_others=" + JSON.stringify(objRest.others));

// Swapping through members.
const swap: any = { a: "A", b: "B" };
[swap.a, swap.b] = [swap.b, swap.a];
console.log("swap=" + swap.a + swap.b);

// A member target whose key is computed from a counter, twice.
const counted: any = {};
let n = 0;
function keyName(): string { return "k" + n++; }
[counted[keyName()], counted[keyName()], counted[keyName()]] = [1, 2, 3];
console.log("computed_keys=" + Object.keys(counted).join(",") + "=" + Object.keys(counted).map((k) => counted[k]).join(""));

// The source is iterated, so a Set or a string works as the right-hand side.
const fromSet: any = {};
[fromSet.x, fromSet.y] = new Set(["s1", "s2", "s3"]);
console.log("from_set=" + fromSet.x + "," + fromSet.y);
const fromStr: any = {};
[fromStr.c0, fromStr.c1] = "hi";
console.log("from_string=" + fromStr.c0 + fromStr.c1);

// Elision skips a source element without touching any target.
trace.length = 0;
const elided: any = {};
[, elided.second, , elided.fourth] = [note("s0", 0), note("s1", 1), note("s2", 2), note("s3", 3)];
console.log("elision_result=" + elided.second + "," + elided.fourth);

// The whole assignment expression evaluates to the right-hand side.
const rhs = [1, 2];
const outTarget: any = {};
const value = ([outTarget.p, outTarget.q] = rhs);
console.log("expression_value_is_rhs=" + (value === rhs));

// An assignment pattern in a `for-of` head writes members each iteration.
const rows: any = { keys: [] as string[], vals: [] as number[] };
const acc: any = {};
for ([acc.k, acc.v] of [["a", 1], ["b", 2]] as any) {
  rows.keys.push(acc.k);
  rows.vals.push(acc.v);
}
console.log("for_of_targets=" + rows.keys.join("") + "/" + rows.vals.join(""));
