// Pins the `Object.create(getPrototypeOf(o), getOwnPropertyDescriptors(o))`
// clone idiom: it preserves accessors, symbols, non-enumerable slots and the
// prototype where spread/assign flatten all four — and it still cannot clone an
// array's exotic length. Also pins the own-key ORDER of a descriptor object.

const proto: any = { fromProto: "P" };
const symKey = Symbol("sk");
let backing = "B0";

const src: any = Object.create(proto);
src.plain = 1;
Object.defineProperty(src, "acc", {
  get() { return "acc:" + backing; },
  set(v: string) { backing = v; },
  enumerable: true,
  configurable: true,
});
Object.defineProperty(src, "hidden", { value: "H", enumerable: false, writable: true, configurable: true });
Object.defineProperty(src, "locked", { value: "L", enumerable: true, writable: false, configurable: false });
src[symKey] = "S";

// the descriptor object itself is an ordinary object with a fixed key order
console.log("desc_data_keys=" + Object.keys(Object.getOwnPropertyDescriptor(src, "plain") as any).join("|"));
console.log("desc_acc_keys=" + Object.keys(Object.getOwnPropertyDescriptor(src, "acc") as any).join("|"));
console.log("desc_proto=" + (Object.getPrototypeOf(Object.getOwnPropertyDescriptor(src, "plain") as any) === Object.prototype));

const all = Object.getOwnPropertyDescriptors(src);
console.log("gopds_keys=" + Reflect.ownKeys(all).map(String).join("|"));
console.log("gopds_own_enumerable=" + Object.keys(all).join("|"));
console.log("gopds_skips_proto=" + ("fromProto" in all));

const clone: any = Object.create(Object.getPrototypeOf(src), all);
console.log("clone_proto=" + (Object.getPrototypeOf(clone) === proto) + ",inherited=" + clone.fromProto);
console.log("clone_keys=" + Reflect.ownKeys(clone).map(String).join("|"));

const cAcc = Object.getOwnPropertyDescriptor(clone, "acc") as any;
console.log("clone_acc_kind=" + ("get" in cAcc ? "accessor" : "data"));
clone.acc = "B1";
console.log("clone_setter_shared=" + backing + ",src_reads=" + src.acc);

const cHidden = Object.getOwnPropertyDescriptor(clone, "hidden") as any;
console.log("clone_hidden=e=" + cHidden.enumerable + ",v=" + cHidden.value);
const cLocked = Object.getOwnPropertyDescriptor(clone, "locked") as any;
console.log("clone_locked=w=" + cLocked.writable + ",c=" + cLocked.configurable);
console.log("clone_sym=" + clone[symKey]);

// spread loses the prototype, the accessor, the non-enumerable slot and the flags
const spread: any = { ...src };
console.log("spread_proto=" + (Object.getPrototypeOf(spread) === Object.prototype));
console.log("spread_keys=" + Reflect.ownKeys(spread).map(String).join("|"));
const sAcc = Object.getOwnPropertyDescriptor(spread, "acc") as any;
console.log("spread_acc_kind=" + ("get" in sAcc ? "accessor" : "data") + ",v=" + sAcc.value);
const sLocked = Object.getOwnPropertyDescriptor(spread, "locked") as any;
console.log("spread_locked=w=" + sLocked.writable + ",c=" + sLocked.configurable);

// assign behaves the same way, and it copies the symbol too
const assigned: any = Object.assign({}, src);
console.log("assign_keys=" + Reflect.ownKeys(assigned).map(String).join("|"));

// getOwnPropertyDescriptor of a MISSING key is undefined; of an inherited one too
console.log("missing=" + Object.getOwnPropertyDescriptor(src, "nope"));
console.log("inherited=" + Object.getOwnPropertyDescriptor(src, "fromProto"));

// the clone of an ARRAY is a plain object: length stops tracking indices
const arr: any = [1, 2, 3];
const arrClone: any = Object.create(Object.getPrototypeOf(arr), Object.getOwnPropertyDescriptors(arr));
console.log("arrclone_isarray=" + Array.isArray(arrClone) + ",len=" + arrClone.length);
arrClone[3] = 4;
console.log("arrclone_len_after=" + arrClone.length + ",real=" + (arr.length));
console.log("arrclone_tag=" + Object.prototype.toString.call(arrClone));
const lenDesc = Object.getOwnPropertyDescriptor(arr, "length") as any;
console.log("arr_len_desc=w=" + lenDesc.writable + ",e=" + lenDesc.enumerable + ",c=" + lenDesc.configurable);

// getOwnPropertyDescriptors on a primitive boxes it first
console.log("string_descs=" + Reflect.ownKeys(Object.getOwnPropertyDescriptors("ab" as any)).join("|"));
console.log("number_descs=" + Reflect.ownKeys(Object.getOwnPropertyDescriptors(5 as any)).length);

// a descriptor value read out of the map is a live reference, not a snapshot
const live: any = { o: { n: 1 } };
const liveDescs: any = Object.getOwnPropertyDescriptors(live);
live.o.n = 2;
console.log("live=" + liveDescs.o.value.n);

// Object.create with an accessor-shaped descriptor that also has a value throws
try {
  Object.create(null, { bad: { value: 1, get() { return 2; } } as any });
  console.log("both=ok");
} catch (e: any) {
  console.log("both=throw:" + e.constructor.name);
}
