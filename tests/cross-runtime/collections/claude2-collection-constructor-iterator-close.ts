// Cross-runtime: IteratorClose during collection construction. When the adder
// throws, or an entry is not shaped as the constructor demands, the SOURCE
// iterator's return() is called before the error leaves — and on a normal,
// exhausted walk it is not called at all.

function source(values: any[], seen: string[]): any {
  let i = 0;
  return {
    [Symbol.iterator]() { seen.push("iter"); return this; },
    next() {
      seen.push("next" + i);
      return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
    },
    return(v: any) { seen.push("return"); return { done: true, value: v }; },
  };
}

function run(label: string, values: any[], build: (it: any) => any): void {
  const seen: string[] = [];
  let outcome: string;
  try {
    const r = build(source(values, seen));
    outcome = "ok:" + (r && r.size !== undefined ? r.size : String(r));
  } catch (e: any) {
    outcome = e.constructor.name;
  }
  console.log(label + "=" + outcome + " [" + seen.join(",") + "]");
}

// --- a clean walk exhausts the iterator and never calls return ---
run("map_ok", [["a", 1], ["b", 2]], (it) => new Map(it));
run("set_ok", [1, 2], (it) => new Set(it));
run("weakset_ok", [{}, {}], (it) => new WeakSet(it) as any);

// --- a Map entry that is not an object closes the iterator ---
run("map_primitive_entry", [["a", 1], 7, ["c", 3]], (it) => new Map(it));
run("map_null_entry", [null], (it) => new Map(it));
run("map_string_entry", ["ab"], (it) => new Map(it));

// --- a WeakSet/WeakMap refusing a value closes it too ---
run("weakset_primitive", [{}, 5], (it) => new WeakSet(it) as any);
run("weakmap_primitive_key", [[{}, 1], [5, 2]], (it) => new WeakMap(it) as any);
run("weakmap_primitive_entry", [7], (it) => new WeakMap(it) as any);

// --- a getter on the entry that throws closes it ---
const boomEntry: any = { get 0() { throw new EvalError("k"); }, get 1() { return 1; } };
run("map_entry_getter_throws", [boomEntry], (it) => new Map(it));

// --- an adder that throws closes it ---
class ThrowingSet extends Set<any> {
  add(v: any): this {
    if (v === 2) throw new RangeError("no");
    return super.add(v);
  }
}
run("subclass_add_throws", [1, 2, 3], (it) => new ThrowingSet(it));

class ThrowingMap extends Map<any, any> {
  set(k: any, v: any): this {
    if (k === "b") throw new RangeError("no");
    return super.set(k, v);
  }
}
run("subclass_set_throws", [["a", 1], ["b", 2]], (it) => new ThrowingMap(it));

// --- a non-callable adder is refused BEFORE the iterator is opened ---
class NoAdder extends Set<any> {}
(NoAdder.prototype as any).add = 42;
run("adder_not_callable", [1, 2], (it) => new NoAdder(it));

class NoSetter extends Map<any, any> {}
(NoSetter.prototype as any).set = "nope";
run("map_setter_not_callable", [["a", 1]], (it) => new NoSetter(it));

// --- with no iterable at all nothing is opened ---
console.log("no_arg_size=" + new Map().size + ":" + new Set().size);
console.log("null_arg_size=" + new Map(null).size + ":" + new Set(null).size);

// --- a source whose return() itself throws: the ORIGINAL error still wins ---
function badReturn(values: any[], seen: string[]): any {
  let i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { seen.push("next"); return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true }; },
    return() { seen.push("return"); throw new URIError("from_return"); },
  };
}
const seenBad: string[] = [];
try { new Map(badReturn([7], seenBad)); console.log("bad_return=no_throw"); }
catch (e: any) { console.log("bad_return=" + e.constructor.name + " [" + seenBad.join(",") + "]"); }

// --- a source with no return() at all is simply left alone ---
function noReturn(values: any[], seen: string[]): any {
  let i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { seen.push("next"); return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true }; },
  };
}
const seenNo: string[] = [];
try { new Map(noReturn([7], seenNo)); console.log("no_return=no_throw"); }
catch (e: any) { console.log("no_return=" + e.constructor.name + " [" + seenNo.join(",") + "]"); }

// --- a return() answering a primitive is refused with a TypeError of its own ---
function primReturn(values: any[]): any {
  let i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() { return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true }; },
    return() { return 5; },
  };
}
try { new Map(primReturn([7])); console.log("prim_return=no_throw"); }
catch (e: any) { console.log("prim_return=" + e.constructor.name); }

// --- generators are closed the same way, and a closed generator stays done ---
let genState = "fresh";
function* gen(): any {
  try {
    genState = "running";
    yield ["a", 1];
    yield 9;
    yield ["c", 3];
  } finally {
    genState = "finalised";
  }
}
const g = gen();
try { new Map(g); console.log("gen_build=no_throw"); }
catch (e: any) { console.log("gen_build=" + e.constructor.name); }
console.log("gen_state=" + genState);
console.log("gen_after=" + JSON.stringify(g.next()));
