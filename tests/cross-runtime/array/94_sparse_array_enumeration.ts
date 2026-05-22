// Cross-runtime compatibility: sparse arrays and enumeration semantics.
const arr = [0, , 2, undefined, 4];

const forIn: string[] = [];
for (const k in arr) {
  forIn.push(k + ":" + String(arr[k as any]));
}

const forOf: string[] = [];
for (const v of arr) {
  forOf.push(String(v));
}

console.log("len=" + arr.length);
console.log("keys=" + Object.keys(arr).join(","));
console.log("forIn=" + forIn.join("|"));
console.log("forOf=" + forOf.join("|"));
console.log("entries=" + Array.from(arr.entries()).map(([i, v]) => i + ":" + String(v)).join("|"));
console.log("json=" + JSON.stringify(arr));
