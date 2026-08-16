// ONE thing: how a BigInt behaves as an IDENTITY. Map and Set keep it distinct
// from the equal Number (SameValueZero compares types), but a property key goes
// through ToPropertyKey, where 1n and 1 collapse onto the same string "1".

const one: any = 1n;

// --- typeof, wrappers and their equalities ---
console.log("typeof_prim=" + typeof one);
console.log("typeof_wrapper=" + typeof Object(one));
console.log("wrapper_tag=" + Object.prototype.toString.call(Object(one)));
console.log("prim_tag=" + Object.prototype.toString.call(one));
console.log("wrapper_valueOf_is_prim=" + (Object(one).valueOf() === 1n));
console.log("wrapper_loose_eq=" + (Object(one) == one));
console.log("wrapper_strict_eq=" + ((Object(one) as any) === one));
console.log("two_wrappers_eq=" + (Object(1n) === Object(1n)));
console.log("wrapper_instanceof=" + (Object(one) instanceof BigInt));
console.log("prim_instanceof=" + ((one as any) instanceof BigInt));
console.log("proto_of_wrapper=" + (Object.getPrototypeOf(Object(one)) === BigInt.prototype));
console.log("Object_is_same=" + Object.is(1n, 1n));
console.log("Object_is_cross_type=" + Object.is(1n, 1));
console.log("Object_is_neg_zero=" + Object.is(-0n, 0n));

// --- BigInt.prototype methods are branded on [[BigIntData]] ---
function call(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}
call("valueOf_prim", () => BigInt.prototype.valueOf.call(1n));
call("valueOf_wrapper", () => BigInt.prototype.valueOf.call(Object(1n)));
call("valueOf_number", () => BigInt.prototype.valueOf.call(1 as any));
call("valueOf_string", () => BigInt.prototype.valueOf.call("1" as any));
call("toString_wrapper", () => BigInt.prototype.toString.call(Object(255n), 16));
call("toString_prim", () => BigInt.prototype.toString.call(255n, 16));
call("toString_number", () => BigInt.prototype.toString.call(255 as any, 16));

// --- crossing to Number loses precision silently above 2^53 ---
console.log("--- Number() precision ---");
console.log("num_2pow60=" + Number(2n ** 60n));
console.log("num_2pow60_exact=" + (Number(2n ** 60n) === 1152921504606846976));
console.log("num_2pow53=" + Number(2n ** 53n));
console.log("num_2pow53_plus1=" + Number(2n ** 53n + 1n));
console.log("collision=" + (Number(2n ** 53n + 1n) === Number(2n ** 53n)));
console.log("roundtrip_lost=" + (BigInt(Number(2n ** 53n + 1n)) === 2n ** 53n + 1n));
console.log("roundtrip_kept=" + (BigInt(Number(2n ** 53n)) === 2n ** 53n));
console.log("num_huge_is_inf=" + (Number(10n ** 400n) === Infinity));
console.log("num_neg_huge=" + (Number(-(10n ** 400n)) === -Infinity));

// --- SameValueZero in Map and Set keeps 1n and 1 apart ---
console.log("--- collections ---");
const m = new Map<any, string>();
m.set(1n, "bigint");
m.set(1, "number");
m.set("1", "string");
console.log("map_size=" + m.size);
console.log("map_get_bigint=" + m.get(1n));
console.log("map_get_number=" + m.get(1));
console.log("map_get_string=" + m.get("1"));
console.log("map_get_other_bigint_instance=" + m.get(BigInt("1")));
console.log("set_size=" + new Set([1n, 1n, 1, "1", -0n, 0n]).size);
console.log("set_has_bigint=" + new Set([1n]).has(1n));
console.log("set_has_number=" + new Set([1n]).has(1 as any));
console.log("includes_bigint=" + [1n, 2n].includes(1n));
console.log("includes_number=" + ([1n, 2n] as any).includes(1));
console.log("indexOf_bigint=" + [1n, 2n].indexOf(2n));

// --- but a property key collapses to the same string ---
console.log("--- property keys ---");
const o: any = {};
o[1n] = "via-bigint";
console.log("read_by_number=" + o[1]);
console.log("read_by_string=" + o["1"]);
o[1] = "via-number";
console.log("read_by_bigint_after=" + o[1n]);
console.log("keys=" + Object.keys(o).join(","));
console.log("key_count=" + Object.keys(o).length);
console.log("key_typeof=" + typeof Object.keys(o)[0]);
console.log("has_own=" + Object.prototype.hasOwnProperty.call(o, "1"));

const arr: any = ["a", "b", "c"];
console.log("array_by_bigint=" + arr[1n]);
arr[3n] = "d";
console.log("array_len_after_bigint=" + arr.length);
console.log("array_join=" + arr.join(","));
console.log("huge_bigint_key=" + Object.keys({ [2n ** 70n as any]: 1 })[0]);
