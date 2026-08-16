// ONE thing: the thisArg parameter that every iteration method accepts, and how
// the callback's `this` resolves for an arrow, a plain function and a bound one.
const marker = { tag: "THIS" };

// A plain callback with an explicit thisArg sees it.
[1].forEach(function (this: any) { console.log("forEach=" + (this === marker)); }, marker);
console.log("map=" + [1].map(function (this: any) { return this === marker; }, marker)[0]);
console.log("filter=" + ([1].filter(function (this: any) { return this === marker; }, marker).length === 1));
console.log("some=" + [1].some(function (this: any) { return this === marker; }, marker));
console.log("every=" + [1].every(function (this: any) { return this === marker; }, marker));
console.log("find=" + ([1].find(function (this: any) { return this === marker; }, marker) === 1));
console.log("findIndex=" + [1].findIndex(function (this: any) { return this === marker; }, marker));
console.log("findLast=" + ([1].findLast(function (this: any) { return this === marker; }, marker) === 1));
console.log("flatMap=" + [1].flatMap(function (this: any) { return [this === marker]; }, marker)[0]);

// An arrow ignores thisArg entirely — it captured its own.
const outer = { tag: "OUTER" };
console.log("arrow=" + [1].map(() => marker !== outer, marker)[0]);

// A bound function ignores thisArg too.
const bound = function (this: any) { return this && this.tag; }.bind(outer);
console.log("bound=" + [1].map(bound as any, marker)[0]);

// reduce takes NO thisArg — the fourth argument is the array.
const reduceArgs: string[] = [];
[1, 2].reduce(function (this: any, acc: any, v: any, i: number, arr: any[]) {
  reduceArgs.push("argc=" + arguments.length + " i=" + i + " isArr=" + Array.isArray(arr));
  return acc + v;
}, 0);
console.log("reduce=" + reduceArgs.join(" | "));

// sort takes no thisArg either.
console.log("sortArgc=" + [2, 1].sort(function () { return arguments.length as any; }).join(","));

// The callback always receives exactly three arguments.
const arity: number[] = [];
[1, 2].forEach(function () { arity.push(arguments.length); });
[1, 2].map(function () { arity.push(arguments.length); return 0; });
[1, 2].filter(function () { arity.push(arguments.length); return true; });
console.log("arity=" + arity.join(","));

// A missing thisArg leaves `this` undefined in strict callbacks and boxed in
// sloppy ones, so the portable probe is whether it is the marker at all.
console.log("noThisArg=" + [1].map(function (this: any) { return this === marker; })[0]);

// The methods that take NO callback ignore extra arguments.
console.log("joinExtra=" + ([1, 2] as any).join(",", "ignored"));
console.log("includesExtra=" + ([1, 2] as any).includes(1, 0, "ignored"));

// A callback that mutates thisArg still sees the same object each call.
const counter = { n: 0 };
[1, 2, 3].forEach(function (this: any) { this.n++; }, counter);
console.log("sharedThis=" + counter.n);

// Every one of them rejects a non-callable before touching the array.
const probe = new Proxy([1, 2], { get(t: any, k) { if (k === "0") console.log("READ"); return t[k]; } });
for (const m of ["forEach", "map", "filter", "some", "every", "find"]) {
  try { (Array.prototype as any)[m].call(probe, undefined); }
  catch (e: any) { console.log(m + "=" + e.constructor.name); }
}
