// Cross-runtime: SameValueZero on Map/Set KEYS — -0 normalizes to +0 on storage,
// NaN is its own key. Goes deep on the *stored key* identity, not just get/has.
// (406 checks get/has surface; this checks what key comes BACK out.)

// --- -0 normalization on the stored key ---
const m1 = new Map<any, string>();
m1.set(-0, "v");
const k1 = [...m1.keys()][0];
console.log("key_is_zero=" + Object.is(k1, 0));
console.log("key_is_negzero=" + Object.is(k1, -0));
console.log("key_1_div=" + (1 / k1)); // Infinity if normalized to +0

// set(-0) then set(0) is the SAME key, value overwritten
const m2 = new Map<any, string>();
m2.set(-0, "first");
m2.set(0, "second");
console.log("m2_size=" + m2.size);
console.log("m2_get_negzero=" + m2.get(-0));
console.log("m2_get_zero=" + m2.get(0));

// reverse order: set(0) first, key stays +0
const m3 = new Map<any, string>();
m3.set(0, "first");
m3.set(-0, "second");
console.log("m3_size=" + m3.size);
console.log("m3_key_1_div=" + (1 / [...m3.keys()][0]));
console.log("m3_get=" + m3.get(0));

// -0 produced by computation, not literal
const m4 = new Map<any, string>();
m4.set(0 * -1, "computed");
console.log("m4_computed_1_div=" + (1 / [...m4.keys()][0]));
console.log("m4_has_zero=" + m4.has(0));

// --- Set: same normalization ---
const s1 = new Set<any>([-0]);
console.log("s1_1_div=" + (1 / [...s1][0]));
console.log("s1_has_zero=" + s1.has(0));
console.log("s1_size_after_add_zero=" + s1.add(0).size);

const s2 = new Set<any>([0, -0, 0 * -1, -(0)]);
console.log("s2_size=" + s2.size);
console.log("s2_1_div=" + (1 / [...s2][0]));

// --- NaN as key: all NaNs are one key ---
const m5 = new Map<any, string>();
m5.set(NaN, "a");
m5.set(Number.NaN, "b");
m5.set(0 / 0, "c");
m5.set(Math.sqrt(-1), "d");
m5.set(parseFloat("zzz"), "e");
console.log("m5_size=" + m5.size);
console.log("m5_get_nan=" + m5.get(NaN));
console.log("m5_key_is_nan=" + Number.isNaN([...m5.keys()][0]));

// NaN !== NaN but Map still finds it
console.log("nan_eq_self=" + (NaN === NaN));
console.log("m5_has_nan=" + m5.has(NaN));
console.log("m5_has_computed_nan=" + m5.has(0 / 0));

const s3 = new Set<any>([NaN, NaN, 0 / 0, Number.NaN]);
console.log("s3_size=" + s3.size);
console.log("s3_has=" + s3.has(NaN));
console.log("s3_delete=" + s3.delete(NaN) + ":" + s3.size);

// --- -0/NaN keys survive forEach + delete ---
const m6 = new Map<any, string>([[-0, "z"], [NaN, "n"]]);
const seen: string[] = [];
m6.forEach((v, k) => { seen.push(String(1 / k) + "=" + v); });
console.log("m6_foreach=" + seen.join("|"));
console.log("m6_delete_negzero=" + m6.delete(-0) + ":" + m6.size);
console.log("m6_delete_nan=" + m6.delete(0 / 0) + ":" + m6.size);
