// Pins Object.assign ABORTING mid-copy: it propagates the first throw from a
// source getter or a target setter and leaves everything copied so far in
// place, and it skips null/undefined sources entirely. The existing assign
// fixtures only cover successful copies and trap ordering.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const order: string[] = [];

// a source getter that throws on the third key
const src: any = {};
Object.defineProperty(src, "a", { get() { order.push("a"); return 1; }, enumerable: true });
Object.defineProperty(src, "b", { get() { order.push("b"); return 2; }, enumerable: true });
Object.defineProperty(src, "boom", { get() { order.push("boom"); throw new RangeError("x"); }, enumerable: true });
Object.defineProperty(src, "c", { get() { order.push("c"); return 3; }, enumerable: true });

const dest: any = {};
attempt("src_throw", () => { Object.assign(dest, src); return "ok"; });
console.log("order=" + order.join("|"));
console.log("partial=" + Object.keys(dest).join("|"));
console.log("partial_values=" + Object.values(dest).join("|"));

// a TARGET setter that throws stops the copy at that key
const order2: string[] = [];
const dest2: any = { plain: 0 };
Object.defineProperty(dest2, "bad", { set() { order2.push("set-bad"); throw new TypeError("y"); }, get() { return "G"; }, enumerable: true, configurable: true });
attempt("dest_throw", () => { Object.assign(dest2, { first: 1, bad: 2, last: 3 }); return "ok"; });
console.log("order2=" + order2.join("|"));
console.log("dest2_keys=" + Object.keys(dest2).join("|"));
console.log("dest2_first=" + dest2.first + ",last=" + dest2.last);

// a later SOURCE is not read at all once an earlier one threw
const readLog: string[] = [];
const s1: any = { get ok() { readLog.push("s1.ok"); return 1; }, get bad() { readLog.push("s1.bad"); throw new Error("z"); } };
const s2: any = { get never() { readLog.push("s2.never"); return 2; } };
attempt("multi", () => { Object.assign({}, s1, s2); return "ok"; });
console.log("readLog=" + readLog.join("|"));

// null and undefined sources are skipped, not an error
console.log("nullish=" + JSON.stringify(Object.assign({ k: 1 }, null, undefined, { j: 2 })));
// a null TARGET is an error
attempt("null_target", () => JSON.stringify(Object.assign(null as any, { a: 1 })));
attempt("undef_target", () => JSON.stringify(Object.assign(undefined as any, { a: 1 })));

// a primitive source is wrapped: only a string contributes own enumerable keys
console.log("str_src=" + JSON.stringify(Object.assign({}, "ab")));
console.log("num_src=" + JSON.stringify(Object.assign({}, 42)));
console.log("bool_src=" + JSON.stringify(Object.assign({}, true)));
console.log("sym_src=" + JSON.stringify(Object.assign({}, Symbol("s") as any)));

// a primitive TARGET is boxed and returned as an object
const boxed: any = Object.assign(7 as any, { tag: "t" });
console.log("boxed_type=" + typeof boxed + ",tag=" + boxed.tag + ",valueOf=" + boxed.valueOf());

// non-enumerable and inherited source properties are never copied
const proto: any = { inherited: "no" };
const mixed: any = Object.create(proto);
mixed.visible = "yes";
Object.defineProperty(mixed, "hidden", { value: "no", enumerable: false });
console.log("filtered=" + JSON.stringify(Object.assign({}, mixed)));

// symbols are copied, and only the enumerable ones
const se = Symbol("enum");
const sh = Symbol("hidden");
const symSrc: any = { [se]: 1 };
Object.defineProperty(symSrc, sh, { value: 2, enumerable: false });
const symDest: any = Object.assign({}, symSrc);
console.log("sym_copied=" + (symDest[se] === 1) + ",sym_skipped=" + (symDest[sh] === undefined));
console.log("sym_keys=" + Object.getOwnPropertySymbols(symDest).map(String).join("|"));

// assign uses Set, so a NON-WRITABLE own property on the target refuses in strict
const locked: any = {};
Object.defineProperty(locked, "ro", { value: "keep", writable: false, enumerable: true, configurable: false });
attempt("locked", () => { Object.assign(locked, { ro: "new" }); return "ok"; });
console.log("locked_value=" + locked.ro);

// but a FROZEN target with nothing to copy is fine
const frozen: any = Object.freeze({ a: 1 });
attempt("frozen_empty", () => { Object.assign(frozen, {}); return "ok"; });
attempt("frozen_nonempty", () => { Object.assign(frozen, { b: 2 }); return "ok"; });

// key order is source order, and a later source wins
console.log("override=" + JSON.stringify(Object.assign({}, { a: 1, b: 2 }, { b: 3, c: 4 }, { a: 5 })));
