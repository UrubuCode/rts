// Cross-runtime: subclassing the collections — the prototype comes from
// NEW.TARGET, and the derived methods that build a new collection do NOT
// consult Symbol.species: they always answer a base Map/Set.

class MyMap extends Map<any, any> {
  tag = "mine";
  first(): any { return [...this.keys()][0]; }
}
class MySet extends Set<any> {
  tag = "mine";
}

// --- an ordinary subclass instance ---
const mm = new MyMap([["a", 1], ["b", 2]]);
console.log("mm_size=" + mm.size);
console.log("mm_tag=" + mm.tag);
console.log("mm_first=" + mm.first());
console.log("mm_instanceof=" + (mm instanceof MyMap) + ":" + (mm instanceof Map));
console.log("mm_proto=" + (Object.getPrototypeOf(mm) === MyMap.prototype));
console.log("mm_ctor_name=" + mm.constructor.name);
console.log("mm_tostring=" + Object.prototype.toString.call(mm));
console.log("mm_own=" + Object.getOwnPropertyNames(mm).join(","));

// --- the class field is installed AFTER super(), so the entries survive ---
console.log("mm_keys=" + [...mm.keys()].join(","));

// --- static inheritance ---
console.log("static_proto=" + (Object.getPrototypeOf(MyMap) === Map));
console.log("subclass_species=" + (MyMap[Symbol.species] === MyMap));
console.log("subclass_groupby=" + (typeof (MyMap as any).groupBy));

// --- Reflect.construct: the prototype follows newTarget, not the callee ---
const rc: any = Reflect.construct(Map, [[["k", 9]]], MyMap);
console.log("rc_proto_is_mymap=" + (Object.getPrototypeOf(rc) === MyMap.prototype));
console.log("rc_instanceof=" + (rc instanceof MyMap) + ":" + (rc instanceof Map));
console.log("rc_size=" + rc.size + ":" + rc.get("k"));
console.log("rc_has_field=" + ("tag" in rc));
console.log("rc_first=" + rc.first());

// --- newTarget with an unrelated constructor still gives a working Map ---
function Alien() { /* never runs — only its .prototype is read */ }
(Alien as any).prototype = { kind: "alien" };
const rc2: any = Reflect.construct(Map, [], Alien as any);
console.log("rc2_kind=" + rc2.kind);
console.log("rc2_is_map=" + (rc2 instanceof Map));
console.log("rc2_has_map_methods=" + (typeof rc2.set));
console.log("rc2_works=" + Map.prototype.set.call(rc2, "z", 1).size);
console.log("rc2_get=" + Map.prototype.get.call(rc2, "z"));

// --- a newTarget whose .prototype is not an object falls back to the default ---
function NoProto() { /* placeholder */ }
(NoProto as any).prototype = 7;
const rc3: any = Reflect.construct(Set, [[1, 2]], NoProto as any);
console.log("rc3_proto_default=" + (Object.getPrototypeOf(rc3) === Set.prototype));
console.log("rc3_size=" + rc3.size);

// --- the Set operations answer a BASE Set, ignoring species ---
const ms = new MySet([1, 2, 3]);
const u = ms.union(new Set([4]));
console.log("union_ctor=" + u.constructor.name);
console.log("union_is_mysubclass=" + (u instanceof MySet));
console.log("union_values=" + [...u].join(","));
const d = ms.difference(new Set([1]));
console.log("difference_ctor=" + d.constructor.name);
console.log("intersection_ctor=" + ms.intersection(new Set([2])).constructor.name);
console.log("symdiff_ctor=" + ms.symmetricDifference(new Set([9])).constructor.name);

// --- an explicit species declaration is still ignored by those methods ---
class SpeciesSet extends Set<any> {
  static get [Symbol.species]() { return Set; }
}
const ss = new SpeciesSet([1]);
console.log("species_declared=" + (SpeciesSet[Symbol.species] === Set));
console.log("species_union_ctor=" + ss.union(new Set([2])).constructor.name);

// --- new Map(subclassInstance) copies through the ordinary iterable path ---
const copy = new Map(mm);
console.log("copy_ctor=" + copy.constructor.name + ":" + copy.size);
console.log("copy_not_same=" + (copy === mm));

// --- a subclass that forgets super() cannot touch `this` ---
class Broken extends Map<any, any> {
  constructor() {
    super();
    // reaching `this` is legal only after super()
  }
}
console.log("broken_ok=" + new Broken().size);

// --- extending without new is still refused ---
try { (MyMap as any)(); console.log("call_subclass=no_throw"); }
catch (e: any) { console.log("call_subclass=" + e.constructor.name); }
