// Pins Reflect.set with a receiver that is NOT the target: the data write lands
// on the receiver via CreateDataProperty (so it ignores the target's attributes
// but is refused by the receiver's own non-writable slot), while an accessor on
// the target runs with `this` bound to the receiver.

const target: any = { a: 1 };
const receiver: any = {};

console.log("set=" + Reflect.set(target, "a", 2, receiver));
console.log("target_a=" + target.a);
console.log("receiver_a=" + receiver.a);
const dr = Object.getOwnPropertyDescriptor(receiver, "a") as any;
console.log("receiver_desc=w=" + dr.writable + ",e=" + dr.enumerable + ",c=" + dr.configurable);

// the receiver gets a fresh property even when the target's is non-enumerable
const t2: any = {};
Object.defineProperty(t2, "hidden", { value: 1, writable: true, enumerable: false, configurable: true });
const r2: any = {};
console.log("set2=" + Reflect.set(t2, "hidden", 5, r2));
const d2 = Object.getOwnPropertyDescriptor(r2, "hidden") as any;
console.log("r2_desc=e=" + d2.enumerable + ",c=" + d2.configurable + ",v=" + d2.value);

// refused when the RECEIVER already has a non-writable own property
const r3: any = {};
Object.defineProperty(r3, "a", { value: "locked", writable: false, configurable: false });
console.log("set3=" + Reflect.set(target, "a", 9, r3));
console.log("r3_a=" + r3.a);

// refused when the TARGET's property is non-writable, before the receiver is consulted
const t4: any = {};
Object.defineProperty(t4, "ro", { value: 1, writable: false, configurable: false });
const r4: any = {};
console.log("set4=" + Reflect.set(t4, "ro", 2, r4));
console.log("r4_has=" + ("ro" in r4));

// refused when the receiver already has an ACCESSOR own property
const r5: any = {};
Object.defineProperty(r5, "a", { get() { return "g"; }, configurable: true });
console.log("set5=" + Reflect.set(target, "a", 3, r5));
console.log("r5_a=" + r5.a);

// a SETTER on the target runs with `this` === receiver
let where = "none";
const t6: any = {};
Object.defineProperty(t6, "s", {
  set(v: string) { where = v + "@" + ((this as any).tag || "?"); },
  get() { return "target-get@" + ((this as any).tag || "?"); },
  configurable: true,
});
t6.tag = "TARGET";
const r6: any = { tag: "RECEIVER" };
console.log("set6=" + Reflect.set(t6, "s", "v", r6));
console.log("where=" + where);
console.log("get6=" + Reflect.get(t6, "s", r6));
console.log("r6_own_s=" + Object.prototype.hasOwnProperty.call(r6, "s"));

// a receiver that is a PRIMITIVE cannot receive the property, so set answers false
console.log("set_prim=" + Reflect.set(target, "a", 4, 7 as any));
console.log("set_prim_str=" + Reflect.set(target, "a", 4, "s" as any));

// a non-extensible receiver refuses a NEW property but accepts an existing one
const r7: any = { a: 0 };
Object.preventExtensions(r7);
console.log("set7_existing=" + Reflect.set(target, "a", 8, r7) + ",a=" + r7.a);
console.log("set7_new=" + Reflect.set(target, "zzz", 8, r7));

// two-argument Reflect.set defaults the receiver to the target
const t8: any = { v: 1 };
console.log("default_recv=" + Reflect.set(t8, "v", 2) + ",v=" + t8.v);

// inherited data property: plain assignment creates an OWN one on the child
const proto: any = { inh: "p" };
const child: any = Object.create(proto);
console.log("inh_set=" + Reflect.set(child, "inh", "c"));
console.log("child_own=" + Object.prototype.hasOwnProperty.call(child, "inh") + ",proto=" + proto.inh);

// inherited NON-WRITABLE data property blocks the child's assignment
const proto2: any = {};
Object.defineProperty(proto2, "ro", { value: "p", writable: false });
const child2: any = Object.create(proto2);
console.log("inh_ro_set=" + Reflect.set(child2, "ro", "c"));
console.log("child2_own=" + Object.prototype.hasOwnProperty.call(child2, "ro") + ",read=" + child2.ro);
