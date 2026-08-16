// ONE thing: how a hole survives — or does not — each serialisation route.
// JSON turns it into null, spread turns it into undefined, and Object.keys
// never sees it at all. Four routes, four different answers.
const a: any[] = [1, , 3, , ];
console.log("len=" + a.length);
console.log("in=" + [0, 1, 2, 3].map((i) => (i in a ? "y" : "n")).join(""));

console.log("json=" + JSON.stringify(a));
console.log("jsonRoundTrip=" + JSON.parse(JSON.stringify(a)).map(String).join(","));
console.log("roundTripIn=" + JSON.parse(JSON.stringify(a)).map((_v: any, i: number, s: any[]) => (i in s ? "y" : "n")).join(""));

const spread = [...a];
console.log("spreadIn=" + [0, 1, 2, 3].map((i) => (i in spread ? "y" : "n")).join("") + " v=" + spread.map(String).join(","));

console.log("keys=" + Object.keys(a).join(","));
console.log("entries=" + Object.entries(a).map((p) => p[0] + ":" + String(p[1])).join(" "));
console.log("ownKeys=" + Reflect.ownKeys(a).map(String).join(","));

const forIn: string[] = [];
for (const k in a) forIn.push(k);
console.log("forIn=" + forIn.join(","));

const forOf: string[] = [];
for (const v of a) forOf.push(String(v));
console.log("forOf=" + forOf.join(","));

console.log("join=" + a.join(",") + "|");
console.log("toString=" + String(a) + "|");
console.log("arrayFrom=" + Array.from(a).map(String).join(","));

// Nested holes: JSON flattens them at every depth.
const nested: any[] = [[1, , 3], { k: undefined }, [, ]];
console.log("nestedJson=" + JSON.stringify(nested));

// A hole inside an object-valued element is an ordinary missing property.
console.log("objUndef=" + JSON.stringify({ a: undefined, b: 1 }));
console.log("objNull=" + JSON.stringify({ a: null, b: 1 }));

// The replacer sees a hole as undefined, with the index as the key.
const seen: string[] = [];
JSON.stringify([1, , 3], function (k, v) { seen.push(k + "=" + String(v)); return v; });
console.log("replacer=" + seen.join(" "));

// A function or symbol element also becomes null in an array.
console.log("fnInArray=" + JSON.stringify([1, function () {}, Symbol("s"), undefined, 2]));
console.log("fnInObject=" + JSON.stringify({ a: 1, f() {}, s: Symbol("s"), u: undefined }));

// A trailing comma never adds an element; two commas do add a hole.
console.log("trailing=" + [1, 2, ].length + " double=" + [1, , 2].length + " leading=" + [, 1].length);
console.log("onlyCommas=" + [, , , ].length);

// structuredClone materialises holes into undefined, like spread.
const sc = structuredClone(a);
console.log("cloneIn=" + [0, 1, 2, 3].map((i) => (i in sc ? "y" : "n")).join("") + " len=" + sc.length);
