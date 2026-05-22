// Cross-runtime compatibility: object property ordering.
const sym = Symbol("z");
const obj: any = {};
obj["20"] = "twenty";
obj["3"] = "three";
obj["aa"] = "aa";
obj["10"] = "ten";
obj["b"] = "b";
obj[sym] = "sym";

console.log("keys=" + Object.keys(obj).join(","));
console.log("names=" + Object.getOwnPropertyNames(obj).join(","));
console.log("symbols=" + Object.getOwnPropertySymbols(obj).length);
console.log("json=" + JSON.stringify(obj));
