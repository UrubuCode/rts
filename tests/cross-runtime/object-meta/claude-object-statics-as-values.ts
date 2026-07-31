// Cross-runtime: `Object`'s static surface must be readable as a VALUE
// (`const d = Object.defineProperty`), not only callable in place.
//
// Regression guard for the WhatsApp-Web bundle campaign (#2038): reading one of
// these bailed with "no such static field on class `Object`" while the CALLED
// form already worked. Minified bundles stash these in a variable constantly
// (`var d = Object.defineProperty`), so the read form is the common one.

console.log("defineProperty=" + typeof Object.defineProperty);
console.log("defineProperties=" + typeof Object.defineProperties);
console.log("fromEntries=" + typeof Object.fromEntries);
console.log("create=" + typeof Object.create);
console.log("setPrototypeOf=" + typeof Object.setPrototypeOf);
console.log("getOwnPropertyDescriptor=" + typeof Object.getOwnPropertyDescriptor);
console.log("getOwnPropertySymbols=" + typeof Object.getOwnPropertySymbols);
console.log("preventExtensions=" + typeof Object.preventExtensions);
console.log("isExtensible=" + typeof Object.isExtensible);

// Being "function" is not enough — the stored value must actually WORK.
const dp = Object.defineProperty;
const target: any = {};
dp(target, "a", { value: 7 });
console.log("definePropertyWorks=" + target.a);

const fe = Object.fromEntries;
console.log("fromEntriesWorks=" + JSON.stringify(fe([["k", 1]])));

const gopd = Object.getOwnPropertyDescriptor;
const d: any = gopd({ a: 5 }, "a");
console.log("descriptorWorks=" + d.value);

// ── non-regressions: the ones that already worked ───────────────────────────
console.log("keys=" + typeof Object.keys);
console.log("assign=" + typeof Object.assign);
const k = Object.keys;
console.log("keysWorks=" + k({ x: 1, y: 2 }).join(","));
