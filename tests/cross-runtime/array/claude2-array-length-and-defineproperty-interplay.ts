// ONE thing: the exotic ArraySetLength algorithm, driven through
// Object.defineProperty rather than assignment — making length non-writable,
// shrinking and redefining at once, and what happens when the deletion loop is
// blocked partway. Probed with Reflect so the answer does not depend on mode.
function len(a: any[]) { return a.length + "[" + Object.keys(a).join(",") + "]"; }

// Redefining length with a smaller value truncates, exactly like assignment.
const a = [0, 1, 2, 3];
console.log("defineSmaller=" + Reflect.defineProperty(a, "length", { value: 2 }) + " " + len(a));

// Making length NON-WRITABLE in the same call that shrinks it: the shrink
// happens first, then the attribute is applied.
const b = [0, 1, 2, 3];
console.log("shrinkAndLock=" + Reflect.defineProperty(b, "length", { value: 1, writable: false }) + " " + len(b));
console.log("lockedDesc=" + JSON.stringify(Object.getOwnPropertyDescriptor(b, "length")));
console.log("pushAfterLock=" + (() => { try { b.push(9); return "no-throw:" + b.length; } catch (e: any) { return e.constructor.name + ":" + b.length; } })());
console.log("setAfterLock=" + Reflect.set(b, "length", 5) + " " + len(b));
console.log("indexWriteAfterLock=" + Reflect.set(b, 1, "x") + " " + len(b));
console.log("index0WriteAfterLock=" + Reflect.set(b, 0, "y") + " v=" + b[0]);

// A non-writable length can still be made writable again? No — it is
// non-configurable, so the attribute is one-way.
console.log("unlock=" + Reflect.defineProperty(b, "length", { writable: true }));
console.log("sameValueRedefine=" + Reflect.defineProperty(b, "length", { value: 1, writable: false }));

// length can never be made enumerable or configurable.
const c = [1];
console.log("makeEnumerable=" + Reflect.defineProperty(c, "length", { enumerable: true }));
console.log("makeConfigurable=" + Reflect.defineProperty(c, "length", { configurable: true }));
console.log("makeAccessor=" + Reflect.defineProperty(c, "length", { get() { return 0; } }));
console.log("cDesc=" + JSON.stringify(Object.getOwnPropertyDescriptor(c, "length")));

// A blocked deletion: length stops one PAST the non-configurable element and
// the operation reports failure.
const d = [0, 1, 2, 3, 4];
Object.defineProperty(d, 2, { value: 22, configurable: false });
console.log("blocked=" + Reflect.defineProperty(d, "length", { value: 0 }) + " " + len(d));
console.log("blockedAgain=" + Reflect.set(d, "length", 1) + " " + len(d));
console.log("blockedGrow=" + Reflect.set(d, "length", 9) + " len=" + d.length);

// Defining an INDEX beyond length grows length; defining one below does not
// shrink it.
const e: any[] = [0];
console.log("defineIndex5=" + Reflect.defineProperty(e, 5, { value: "five", enumerable: true, writable: true, configurable: true }) + " " + len(e));
console.log("defineIndex0=" + Reflect.defineProperty(e, 0, { value: "zero" }) + " " + len(e));

// A non-index numeric-looking key never touches length.
console.log("defineStrKey=" + Reflect.defineProperty(e, "007", { value: "bond", enumerable: true }) + " " + len(e));
console.log("defineFloatKey=" + Reflect.defineProperty(e, "1.5", { value: "half", enumerable: true }) + " " + len(e));

// An index defined as an ACCESSOR still counts for length and for iteration.
const f: any[] = [];
Reflect.defineProperty(f, 0, { get() { return "computed"; }, enumerable: true, configurable: true });
console.log("accessorIndex=" + len(f) + " v=" + f[0] + " join=" + f.join(",") + " json=" + JSON.stringify(f));
console.log("accessorMap=" + JSON.stringify(f.map((x: any) => x)));

// A non-enumerable index is skipped by Object.keys but NOT by the array methods.
const g: any[] = [1, 2];
Reflect.defineProperty(g, 1, { enumerable: false });
console.log("nonEnumIndex=" + len(g) + " join=" + g.join(",") + " json=" + JSON.stringify(g));
console.log("nonEnumForEach=" + (() => { let n = 0; g.forEach(() => n++); return n; })());
console.log("nonEnumForIn=" + (() => { const o: string[] = []; for (const k in g) o.push(k); return o.join(","); })());
