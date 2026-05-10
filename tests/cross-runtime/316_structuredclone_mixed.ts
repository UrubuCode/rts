// Cross-runtime: structuredClone with Map/Set/Date/RegExp and identity.
const original = {
  map: new Map([["a", 1]]),
  set: new Set([2, 3]),
  date: new Date(Date.UTC(2024, 4, 10)),
  re: /ab+/gi,
};
const copy = structuredClone(original);
console.log("same=" + String(copy !== original));
console.log("map=" + Array.from(copy.map.entries()).map(([k, v]) => k + ":" + v).join(","));
console.log("set=" + Array.from(copy.set.values()).join(","));
console.log("date=" + copy.date.toISOString());
console.log("re=" + copy.re.source + ":" + copy.re.flags);
