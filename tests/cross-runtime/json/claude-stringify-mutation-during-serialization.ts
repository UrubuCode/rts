// Cross-runtime: JSON.stringify snapshots an object's OWN ENUMERABLE KEY LIST
// once, before visiting any of them — so a key added by a getter running
// mid-serialisation is not picked up, while a deleted one is skipped.

// --- a getter that ADDS a later key: the key list was already taken ---
const adder: any = { a: 1 };
Object.defineProperty(adder, "b", {
  get() { adder.zzz = "late"; return 2; },
  enumerable: true,
  configurable: true,
});
adder.c = 3;
console.log("adder_out=" + JSON.stringify(adder));
console.log("adder_has_zzz=" + ("zzz" in adder));
console.log("adder_second_pass=" + JSON.stringify(adder));

// --- a getter that DELETES a key not yet reached: the read gives undefined,
//     so the key is elided from the output ---
const deleter: any = { a: 1 };
Object.defineProperty(deleter, "b", {
  get() { delete deleter.c; return 2; },
  enumerable: true,
  configurable: true,
});
deleter.c = 3;
deleter.d = 4;
console.log("deleter_out=" + JSON.stringify(deleter));
console.log("deleter_keys_after=" + Object.keys(deleter).join(","));

// --- a getter that CHANGES a later value: the new value is what is written ---
const changer: any = { a: 1 };
Object.defineProperty(changer, "b", {
  get() { changer.c = "changed"; return 2; },
  enumerable: true,
  configurable: true,
});
changer.c = "original";
console.log("changer_out=" + JSON.stringify(changer));

// --- a getter that changes an EARLIER value: already written, so no effect ---
const backwards: any = { a: "first" };
Object.defineProperty(backwards, "b", {
  get() { backwards.a = "mutated"; return 2; },
  enumerable: true,
  configurable: true,
});
console.log("backwards_out=" + JSON.stringify(backwards));
console.log("backwards_a_now=" + backwards.a);

// --- a getter is invoked exactly once per serialisation ---
let reads = 0;
const counted: any = {};
Object.defineProperty(counted, "v", { get() { reads++; return reads; }, enumerable: true });
console.log("counted1=" + JSON.stringify(counted) + ":reads=" + reads);
console.log("counted2=" + JSON.stringify(counted) + ":reads=" + reads);
console.log("counted_repeated=" + JSON.stringify([counted, counted]));

// --- a NON-enumerable property is never read ---
let hiddenReads = 0;
const hidden: any = { shown: 1 };
Object.defineProperty(hidden, "secret", { get() { hiddenReads++; return 9; }, enumerable: false });
console.log("hidden_out=" + JSON.stringify(hidden) + ":reads=" + hiddenReads);

// --- an INHERITED enumerable property is never serialised ---
const base: any = { inherited: "no" };
const derived: any = Object.create(base);
derived.own = "yes";
console.log("inherited_out=" + JSON.stringify(derived));
console.log("inherited_visible=" + derived.inherited);

// --- arrays go by LENGTH, read fresh, so shrinking mid-flight shows as null ---
const arr: any = [0, 1, 2, 3];
Object.defineProperty(arr, "1", {
  get() { arr.length = 2; return "one"; },
  configurable: true,
  enumerable: true,
});
console.log("array_shrunk=" + JSON.stringify(arr));
console.log("array_len_after=" + arr.length);

// --- growing an array mid-flight: length was read once, up front ---
const grow: any = [0, 1];
Object.defineProperty(grow, "0", {
  get() { grow.push(99, 100); return "zero"; },
  configurable: true,
  enumerable: true,
});
console.log("array_grown=" + JSON.stringify(grow));
console.log("array_grown_len=" + grow.length);

// --- a getter that throws aborts the whole serialisation ---
const thrower: any = { a: 1 };
Object.defineProperty(thrower, "b", { get() { throw new RangeError("no"); }, enumerable: true });
try { JSON.stringify(thrower); console.log("thrower=no_throw"); }
catch (e: any) { console.log("thrower=" + e.constructor.name); }
console.log("thrower_a_still=" + thrower.a);

// --- key ORDER is the ordinary own-key order: integer-like first ---
const order: any = {};
order.b = 1; order["2"] = 2; order.a = 3; order["1"] = 4; order["01"] = 5;
console.log("order_out=" + JSON.stringify(order));

// --- a toJSON that mutates the holder ---
const holder: any = { keep: 1, item: { toJSON() { holder.keep = 999; return "done"; } }, tail: 2 };
console.log("tojson_mutates=" + JSON.stringify(holder));
console.log("tojson_keep_now=" + holder.keep);
