// Pins [[PreventExtensions]] as the thing that also LOCKS the prototype: a
// non-extensible object refuses setPrototypeOf to a different value but accepts
// the one it already has, including null-to-null. 153 only checks that new
// properties are refused.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const protoA: any = { tag: "A" };
const protoB: any = { tag: "B" };

const o: any = Object.create(protoA);
o.own = 1;
Object.preventExtensions(o);

console.log("extensible=" + Object.isExtensible(o));
console.log("sealed=" + Object.isSealed(o) + ",frozen=" + Object.isFrozen(o));
console.log("ref_setproto_same=" + Reflect.setPrototypeOf(o, protoA));
console.log("ref_setproto_other=" + Reflect.setPrototypeOf(o, protoB));
attempt("obj_setproto_other", () => { Object.setPrototypeOf(o, protoB); return "ok"; });
console.log("proto_intact=" + o.tag);
console.log("ref_setproto_null=" + Reflect.setPrototypeOf(o, null));

// a null-prototype non-extensible object accepts null and refuses anything else
const bare: any = Object.create(null);
Object.preventExtensions(bare);
console.log("bare_null=" + Reflect.setPrototypeOf(bare, null));
console.log("bare_other=" + Reflect.setPrototypeOf(bare, protoA));

// existing properties stay writable, configurable and deletable
console.log("write=" + Reflect.set(o, "own", 2) + ",v=" + o.own);
console.log("redefine=" + Reflect.defineProperty(o, "own", { value: 3, enumerable: false }));
console.log("delete=" + Reflect.deleteProperty(o, "own"));
console.log("keys_after=" + Object.getOwnPropertyNames(o).join("|"));
// and once deleted it cannot come back
console.log("readd=" + Reflect.set(o, "own", 4) + ",has=" + Object.hasOwn(o, "own"));
console.log("readd_define=" + Reflect.defineProperty(o, "own", { value: 4 }));

// isSealed/isFrozen are computed, not stored: an emptied non-extensible object is both
console.log("empty_sealed=" + Object.isSealed(o) + ",frozen=" + Object.isFrozen(o));

// preventExtensions is idempotent and returns its argument
const q: any = { a: 1 };
console.log("returns_same=" + (Object.preventExtensions(q) === q));
console.log("twice=" + (Object.preventExtensions(q) === q) + ",ext=" + Object.isExtensible(q));

// primitives: Object.* tolerates them, Reflect.* does not
console.log("obj_prevent_num=" + (Object.preventExtensions(7 as any) as any));
console.log("obj_isext_num=" + Object.isExtensible(7 as any));
attempt("ref_prevent_num", () => String(Reflect.preventExtensions(7 as any)));
attempt("ref_isext_num", () => String(Reflect.isExtensible(7 as any)));
attempt("obj_prevent_null", () => String(Object.preventExtensions(null as any)));

// an ARRAY: preventExtensions stops growth but length may still shrink
const arr: any = [1, 2, 3];
Object.preventExtensions(arr);
console.log("arr_push=" + (() => { try { arr.push(4); return "ok"; } catch (e: any) { return "throw:" + e.constructor.name; } })());
console.log("arr_len=" + arr.length + ",json=" + JSON.stringify(arr));
arr.length = 1;
console.log("arr_shrunk=" + JSON.stringify(arr) + ",len=" + arr.length);
console.log("arr_regrow=" + Reflect.set(arr, "1", 9) + ",json=" + JSON.stringify(arr));
console.log("arr_sealed=" + Object.isSealed(arr) + ",frozen=" + Object.isFrozen(arr));

// a SEALED array is not frozen while its elements are writable
const sealedArr: any = Object.seal([1, 2]);
console.log("seal_write=" + Reflect.set(sealedArr, "0", 9) + ",json=" + JSON.stringify(sealedArr));
console.log("seal_len=" + (Object.getOwnPropertyDescriptor(sealedArr, "length") as any).writable);
console.log("seal_isfrozen=" + Object.isFrozen(sealedArr));
const frozenArr: any = Object.freeze([1, 2]);
console.log("freeze_len_writable=" + (Object.getOwnPropertyDescriptor(frozenArr, "length") as any).writable);

// setPrototypeOf on an extensible object is unrestricted, including cycles
const x: any = {};
const y: any = Object.create(x);
console.log("normal_set=" + Reflect.setPrototypeOf(x, protoB) + ",tag=" + x.tag);
attempt("cycle", () => { Object.setPrototypeOf(x, y); return "ok"; });
console.log("ref_cycle=" + Reflect.setPrototypeOf(x, y));
console.log("self=" + Reflect.setPrototypeOf(x, x));

// setPrototypeOf demands an object or null
attempt("proto_number", () => { Object.setPrototypeOf({} as any, 7 as any); return "ok"; });
attempt("proto_undefined", () => { Object.setPrototypeOf({} as any, undefined as any); return "ok"; });
console.log("ref_proto_number=" + (() => { try { return String(Reflect.setPrototypeOf({} as any, 7 as any)); } catch (e: any) { return "throw:" + e.constructor.name; } })());

// Object.setPrototypeOf on a primitive is the identity when the proto is legal
console.log("prim_ok=" + (Object.setPrototypeOf(7 as any, Number.prototype) as any));
attempt("prim_bad", () => String(Object.setPrototypeOf(null as any, null)));
