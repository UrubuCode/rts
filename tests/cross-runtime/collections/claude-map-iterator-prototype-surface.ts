// Cross-runtime: the %MapIteratorPrototype% / %SetIteratorPrototype% objects —
// what keys()/values()/entries() actually hand back, and how those iterator
// objects sit in the prototype chain.

const m = new Map([["a", 1], ["b", 2]]);
const s = new Set([10, 20]);

const mk = m.keys();
const mv = m.values();
const me = m.entries();
const sv = s.values();

// --- the tag ---
console.log("map_iter_tag=" + Object.prototype.toString.call(mk));
console.log("set_iter_tag=" + Object.prototype.toString.call(sv));
console.log("array_iter_tag=" + Object.prototype.toString.call([].values()));

// --- the three Map iterators share one prototype ---
const mkp = Object.getPrototypeOf(mk);
console.log("keys_values_same_proto=" + (mkp === Object.getPrototypeOf(mv)));
console.log("keys_entries_same_proto=" + (mkp === Object.getPrototypeOf(me)));
console.log("map_set_iter_proto_same=" + (mkp === Object.getPrototypeOf(sv)));

// --- but each call returns a NEW iterator object ---
console.log("distinct_objects=" + (m.keys() === m.keys()));
console.log("proto_is_not_iterator=" + (mkp === mk));

// --- %IteratorPrototype% is shared with array iterators ---
const iterProto = Object.getPrototypeOf(mkp);
console.log("shared_iterator_proto=" + (iterProto === Object.getPrototypeOf(Object.getPrototypeOf([].values()))));
console.log("shared_with_string=" + (iterProto === Object.getPrototypeOf(Object.getPrototypeOf(""[Symbol.iterator]()))));
console.log("iterproto_parent_is_object=" + (Object.getPrototypeOf(iterProto) === Object.prototype));

// --- the iterator is itself iterable, and returns ITSELF ---
console.log("self_iterable=" + ((mk as any)[Symbol.iterator]() === mk));
console.log("iterator_fn_on_shared=" + Object.prototype.hasOwnProperty.call(iterProto, Symbol.iterator));
console.log("next_own_here=" + Object.prototype.hasOwnProperty.call(mkp, "next"));

// --- next() result shape ---
const r1: any = mk.next();
console.log("r1=" + r1.value + ":" + r1.done);
console.log("r1_own=" + Object.getOwnPropertyNames(r1).sort().join(","));
console.log("r1_proto_is_object=" + (Object.getPrototypeOf(r1) === Object.prototype));
const r2: any = mk.next();
console.log("r2=" + r2.value + ":" + r2.done);
const r3: any = mk.next();
console.log("r3=" + String(r3.value) + ":" + r3.done);
const r4: any = mk.next();
console.log("r4_after_done=" + String(r4.value) + ":" + r4.done);
console.log("results_distinct=" + (r1 === r2));

// --- entries yields a fresh two-element array each time ---
const e1: any = me.next().value;
console.log("entry_is_array=" + Array.isArray(e1) + ":" + e1.length + ":" + e1.join(":"));
console.log("entry_proto=" + (Object.getPrototypeOf(e1) === Array.prototype));

// --- Set values(): key and value are the same ---
const se: any = s.entries().next().value;
console.log("set_entry=" + se.join(":") + ":same=" + (se[0] === se[1]));

// --- next() has a brand check of its own ---
try { (mkp as any).next.call({}); console.log("next_plain=no_throw"); }
catch (e: any) { console.log("next_plain=" + e.constructor.name); }
try { (mkp as any).next.call(m); console.log("next_on_map=no_throw"); }
catch (e: any) { console.log("next_on_map=" + e.constructor.name); }
try { (mkp as any).next.call([].values()); console.log("next_on_arrayiter=no_throw"); }
catch (e: any) { console.log("next_on_arrayiter=" + e.constructor.name); }

// --- an exhausted iterator does not resurrect when the map grows ---
const g = new Map([["x", 1]]);
const gi = g.keys();
console.log("g1=" + gi.next().value);
console.log("g_done=" + gi.next().done);
g.set("y", 2);
console.log("g_after_grow=" + String(gi.next().value) + ":" + gi.next().done);

// --- a LIVE (not yet exhausted) iterator does see a later addition ---
const h = new Map([["x", 1]]);
const hi = h.keys();
console.log("h1=" + hi.next().value);
h.set("y", 2);
console.log("h2=" + String(hi.next().value));
console.log("h_done=" + hi.next().done);

// --- no return/throw method on collection iterators ---
console.log("has_return=" + (typeof (mkp as any).return));
console.log("has_throw=" + (typeof (mkp as any).throw));
