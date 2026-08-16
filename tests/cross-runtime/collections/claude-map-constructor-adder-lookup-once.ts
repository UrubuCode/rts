// Cross-runtime: the Map/Set constructors read the adder (`set` / `add`) from
// the NEW instance exactly ONCE, then call it for every entry — so a patched
// prototype method or a subclass override participates in construction.

// --- the adder is read once, not once per entry ---
let mapGets = 0;
class CountingMap extends Map<any, any> {
  get set(): any {
    mapGets++;
    return Map.prototype.set;
  }
}
const cm = new CountingMap([["a", 1], ["b", 2], ["c", 3]]);
console.log("map_adder_gets=" + mapGets);
console.log("map_entries=" + [...cm.keys()].join(","));

let setGets = 0;
class CountingSet extends Set<any> {
  get add(): any {
    setGets++;
    return Set.prototype.add;
  }
}
const cs = new CountingSet([1, 2, 3, 4]);
console.log("set_adder_gets=" + setGets);
console.log("set_entries=" + [...cs].join(","));

// --- an empty iterable still reads the adder ---
mapGets = 0;
const cmEmpty = new CountingMap([]);
console.log("map_adder_gets_empty=" + mapGets + ":size=" + cmEmpty.size);

// --- no argument at all: nothing to add, so nothing is read ---
mapGets = 0;
const cmNone = new CountingMap();
console.log("map_adder_gets_noarg=" + mapGets + ":size=" + cmNone.size);

// --- a non-callable adder is refused, even for an empty iterable ---
class BadMap extends Map<any, any> {
  get set(): any { return 42; }
}
try {
  const bad = new BadMap([["a", 1]]);
  console.log("bad_adder=no_throw:" + bad.size);
} catch (e: any) {
  console.log("bad_adder=" + e.constructor.name);
}
try {
  const bad = new BadMap([]);
  console.log("bad_adder_empty=no_throw:" + bad.size);
} catch (e: any) {
  console.log("bad_adder_empty=" + e.constructor.name);
}
try {
  const bad = new BadMap();
  console.log("bad_adder_noarg=no_throw:" + bad.size);
} catch (e: any) {
  console.log("bad_adder_noarg=" + e.constructor.name);
}

// --- a subclass override is what the constructor calls ---
class UpperMap extends Map<any, any> {
  set(k: any, v: any): any {
    return super.set(String(k).toUpperCase(), v);
  }
}
const um = new UpperMap([["a", 1], ["b", 2]]);
console.log("upper_keys=" + [...um.keys()].join(","));
console.log("upper_size=" + um.size);
um.set("c", 3);
console.log("upper_after_set=" + [...um.keys()].join(","));

class DoublingSet extends Set<any> {
  add(v: any): any { return super.add(v * 2); }
}
const ds = new DoublingSet([1, 2, 3]);
console.log("doubled=" + [...ds].join(","));

// --- the adder is invoked with the instance as `this` ---
let sawThis = "";
class ThisMap extends Map<any, any> {
  set(k: any, v: any): any {
    sawThis = (this instanceof ThisMap) + ":" + (this instanceof Map);
    return super.set(k, v);
  }
}
const tm = new ThisMap([["k", "v"]]);
console.log("adder_this=" + sawThis);

// --- an OWN adder on the instance beats the prototype's, and is what the
//     constructor of a further copy uses ---
class OwnAdder extends Map<any, any> {
  constructor(src?: any) {
    super();
    (this as any).set = function (k: any, v: any) { return Map.prototype.set.call(this, "own:" + k, v); };
    if (src) for (const e of src) (this as any).set(e[0], e[1]);
  }
}
const oa = new OwnAdder([["p", 1]]);
console.log("own_adder_keys=" + [...oa.keys()].join(","));
console.log("own_adder_is_own=" + Object.prototype.hasOwnProperty.call(oa, "set"));

// --- the adder is called once per entry, in iteration order ---
const seq: string[] = [];
class SeqSet extends Set<any> {
  add(v: any): any { seq.push(String(v)); return super.add(v); }
}
const seqSet = new SeqSet(["c", "a", "b", "a"]);
console.log("adder_sequence=" + seq.join(","));
console.log("adder_call_count=" + seq.length + ":size=" + seqSet.size);
console.log("stored_order=" + [...seqSet].join(","));

// --- an adder that throws aborts construction ---
class ThrowingSet extends Set<any> {
  add(v: any): any { if (v === 2) throw new RangeError("no"); return super.add(v); }
}
try { const ts = new ThrowingSet([1, 2, 3]); console.log("throwing_adder=no_throw:" + ts.size); }
catch (e: any) { console.log("throwing_adder=" + e.constructor.name); }

// --- patching the shared prototype affects plain construction too ---
const realAdd = Set.prototype.add;
const seen: string[] = [];
(Set.prototype as any).add = function (v: any) {
  seen.push(String(v));
  return realAdd.call(this, v);
};
const plain = new Set([7, 8, 9]);
(Set.prototype as any).add = realAdd;
console.log("patched_seen=" + seen.join(","));
console.log("patched_result=" + [...plain].join(","));
console.log("prototype_restored=" + (Set.prototype.add === realAdd));
