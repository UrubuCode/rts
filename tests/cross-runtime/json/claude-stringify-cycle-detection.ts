// Cross-runtime: cycle detection in JSON.stringify — a repeated NON-cyclic
// reference is fine however many times it appears; only a reference back into
// the stack currently being serialised is a TypeError.

function attempt(label: string, fn: () => any): void {
  try { console.log(label + "=" + String(fn())); }
  catch (e: any) { console.log(label + "=" + e.constructor.name); }
}

// --- a shared child appearing twice is NOT a cycle ---
const shared = { n: 1 };
attempt("shared_twice", () => JSON.stringify({ a: shared, b: shared }));
attempt("shared_in_array", () => JSON.stringify([shared, shared, shared]));
const diamond = { left: { v: shared }, right: { v: shared } };
attempt("diamond", () => JSON.stringify(diamond));

// --- a shared ARRAY appearing twice is fine too ---
const sharedArr = [1, 2];
attempt("shared_array", () => JSON.stringify({ a: sharedArr, b: sharedArr }));

// --- deep repetition of the same leaf ---
const leaf = { x: 0 };
attempt("repeated_leaf", () => JSON.stringify([{ leaf }, { leaf }, [leaf, leaf]]));

// --- direct self-reference on an object ---
const selfObj: any = { a: 1 };
selfObj.me = selfObj;
attempt("object_self", () => JSON.stringify(selfObj));

// --- direct self-reference on an array ---
const selfArr: any = [1, 2];
selfArr.push(selfArr);
attempt("array_self", () => JSON.stringify(selfArr));

// --- indirect cycle through two objects ---
const p: any = {};
const q: any = { p };
p.q = q;
attempt("indirect_cycle", () => JSON.stringify(p));

// --- a longer chain ---
const c1: any = {}; const c2: any = { c1 }; const c3: any = { c2 };
c1.c3 = c3;
attempt("long_cycle", () => JSON.stringify(c3));

// --- a cycle a toJSON INTRODUCES: the wrapper it returns points back at an
//     ancestor that is still on the serialisation stack ---
const parent: any = { name: "p" };
const child: any = { toJSON() { return { up: parent }; } };
parent.child = child;
attempt("cycle_via_tojson", () => JSON.stringify(parent));
attempt("tojson_wrapper_alone", () => JSON.stringify(child));

// --- a cycle the REPLACER introduces ---
const plain: any = { a: 1 };
attempt("cycle_via_replacer", () => JSON.stringify(plain, function (k: any, v: any) {
  return k === "a" ? plain : v;
}));

// --- a cycle the replacer BREAKS is serialised fine ---
attempt("cycle_broken_by_replacer", () => JSON.stringify(selfObj, function (k: any, v: any) {
  return k === "me" ? "<self>" : v;
}));

// --- a cycle toJSON removes is fine ---
const cut: any = { a: 1, toJSON() { return { a: this.a }; } };
cut.self = cut;
attempt("cycle_cut_by_tojson", () => JSON.stringify(cut));

// --- the engine recovers: the SAME object serialises after the throw once the
//     cycle is gone ---
attempt("before_fix", () => JSON.stringify(selfObj));
delete selfObj.me;
attempt("after_fix", () => JSON.stringify(selfObj));

// --- a cycle detected inside an array element ---
const holder: any = { list: [] };
holder.list.push(holder);
attempt("cycle_in_array_element", () => JSON.stringify(holder));

// --- self-reference under a key that comes AFTER a valid one ---
const late: any = { first: "ok", second: 2 };
late.third = late;
attempt("late_cycle", () => JSON.stringify(late));

// --- a Map/Set holding the cycle serialises as {} because they have no own keys ---
const m = new Map<any, any>();
m.set("self", m);
attempt("map_cycle_is_empty", () => JSON.stringify(m));
attempt("map_in_object", () => JSON.stringify({ m }));

// --- a getter returning the parent is still a cycle ---
const getterCycle: any = { plain: 1 };
Object.defineProperty(getterCycle, "up", { get() { return getterCycle; }, enumerable: true });
attempt("getter_cycle", () => JSON.stringify(getterCycle));

// --- a getter returning a FRESH object each time is not ---
const fresh: any = { plain: 1 };
Object.defineProperty(fresh, "made", { get() { return { made: true }; }, enumerable: true });
attempt("fresh_getter", () => JSON.stringify([fresh, fresh]));

// --- depth without a cycle is fine ---
let deep: any = { v: 0 };
for (let i = 1; i <= 12; i++) deep = { v: i, inner: deep };
attempt("deep_no_cycle", () => JSON.stringify(deep).length);
