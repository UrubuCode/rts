// Cross-runtime compatibility: NaN and negative zero semantics.
console.log("is_nan=" + Object.is(NaN, NaN));
console.log("is_neg0=" + Object.is(-0, 0));
console.log("div_neg0=" + String(1 / -0));
console.log("sign_neg0=" + String(Math.sign(-0)));

const map = new Map();
map.set(NaN, "nan");
map.set(-0, "neg");
console.log("map_nan=" + map.get(NaN));
console.log("map_zero=" + map.get(0));

const set = new Set([NaN, NaN, -0, 0]);
console.log("set_size=" + set.size);
