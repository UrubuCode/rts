// Pins what freeze does NOT stop: an accessor's setter still runs on a frozen
// object, because freeze only clears `writable` on DATA properties. Also pins
// Reflect.set answering false where a strict assignment throws.
// 48/152/153 cover freezing plain data objects only.

let sideEffect = "none";
const o: any = { data: 1 };
Object.defineProperty(o, "acc", {
  get() { return "acc:" + sideEffect; },
  set(v: string) { sideEffect = v; },
  enumerable: true,
  configurable: true,
});

Object.freeze(o);
console.log("frozen=" + Object.isFrozen(o));
console.log("sealed=" + Object.isSealed(o));
console.log("extensible=" + Object.isExtensible(o));

const dAcc = Object.getOwnPropertyDescriptor(o, "acc") as any;
console.log("acc_desc=get:" + typeof dAcc.get + ",set:" + typeof dAcc.set + ",c=" + dAcc.configurable + ",e=" + dAcc.enumerable);
console.log("acc_has_writable=" + ("writable" in dAcc));
const dData = Object.getOwnPropertyDescriptor(o, "data") as any;
console.log("data_desc=w=" + dData.writable + ",c=" + dData.configurable);

// the setter RUNS despite the freeze, and the write reports success
console.log("reflect_set_acc=" + Reflect.set(o, "acc", "written"));
console.log("side=" + sideEffect);
console.log("acc_read=" + o.acc);

// the data property does not move
console.log("reflect_set_data=" + Reflect.set(o, "data", 99));
console.log("data=" + o.data);
console.log("reflect_set_new=" + Reflect.set(o, "brandnew", 1));
console.log("has_new=" + ("brandnew" in o));
console.log("reflect_delete=" + Reflect.deleteProperty(o, "data"));

// the refusal is observable as a boolean plus the unchanged value, without
// depending on whether the caller's mode turns it into a throw
console.log("refused_data=" + Reflect.set(o, "data", 5) + ",still=" + o.data);
console.log("refused_new=" + Reflect.set(o, "zzz", 5) + ",still=" + ("zzz" in o));
console.log("accepted_acc=" + Reflect.set(o, "acc", "again"));
console.log("side2=" + sideEffect);
console.log("data_desc_unchanged=" + (Object.getOwnPropertyDescriptor(o, "data") as any).value);

// seal keeps writes working, refuses additions and deletions
const s: any = { a: 1 };
Object.seal(s);
console.log("seal_frozen=" + Object.isFrozen(s) + ",sealed=" + Object.isSealed(s));
console.log("seal_write=" + Reflect.set(s, "a", 2) + ",a=" + s.a);
console.log("seal_add=" + Reflect.set(s, "b", 1));
console.log("seal_delete=" + Reflect.deleteProperty(s, "a"));
console.log("seal_redefine=" + Reflect.defineProperty(s, "a", { value: 3, writable: false }));
console.log("seal_a_after=" + s.a);

// an object with only accessors is considered FROZEN once non-extensible and
// non-configurable, because there is no writable attribute to clear
const onlyAcc: any = {};
Object.defineProperty(onlyAcc, "x", { get() { return 1; }, set() { /* noop */ }, configurable: false });
Object.preventExtensions(onlyAcc);
console.log("onlyacc_frozen=" + Object.isFrozen(onlyAcc));
console.log("onlyacc_sealed=" + Object.isSealed(onlyAcc));

// freeze is shallow
const outer: any = { inner: { v: 1 } };
Object.freeze(outer);
outer.inner.v = 2;
console.log("shallow=" + outer.inner.v);
console.log("inner_frozen=" + Object.isFrozen(outer.inner));

// freeze on a primitive is the identity, and isFrozen answers true for one
console.log("freeze_prim=" + (Object.freeze(7) as any));
console.log("isfrozen_prim=" + Object.isFrozen(7) + "," + Object.isFrozen("abc"));

// an empty non-extensible object is both sealed and frozen
const empty: any = {};
Object.preventExtensions(empty);
console.log("empty=" + Object.isSealed(empty) + "," + Object.isFrozen(empty));
