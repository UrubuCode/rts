// ONE thing: how an array turns into a primitive. It has no Symbol.toPrimitive,
// so OrdinaryToPrimitive runs valueOf (which answers the array itself, an
// object, so it is rejected) and then toString — and that path is observable.
const calls: string[] = [];
const a: any = [1, 2];
a.valueOf = function () { calls.push("valueOf"); return this; };
a.toString = function () { calls.push("toString"); return "STR"; };

console.log("plus=" + (a + ""));
console.log("order=" + calls.join(","));

calls.length = 0;
console.log("template=" + `${a}`);
console.log("templateOrder=" + calls.join(","));

calls.length = 0;
console.log("numeric=" + Number(a));
console.log("numericOrder=" + calls.join(","));

// A valueOf answering a PRIMITIVE wins and toString is never reached.
const b: any = [1, 2];
const bCalls: string[] = [];
b.valueOf = function () { bCalls.push("valueOf"); return 42; };
b.toString = function () { bCalls.push("toString"); return "NO"; };
console.log("primValueOf=" + (b + 1) + " order=" + bCalls.join(","));
console.log("primString=" + String(b) + " order=" + bCalls.join(","));

// String() uses the STRING hint, so toString comes first.
const c: any = [1];
const cCalls: string[] = [];
c.valueOf = function () { cCalls.push("valueOf"); return 7; };
c.toString = function () { cCalls.push("toString"); return "C"; };
console.log("String=" + String(c) + " order=" + cCalls.join(","));
cCalls.length = 0;
console.log("bracket=" + ({ C: "hit" } as any)[c] + " order=" + cCalls.join(","));

// A Symbol.toPrimitive on the array overrides both.
const d: any = [1];
d[Symbol.toPrimitive] = function (hint: string) { return "hint:" + hint; };
console.log("sp_default=" + (d + ""));
console.log("sp_string=" + String(d));
console.log("sp_number=" + Number(d));

// The default array toString is join, so nested arrays flatten and null and
// undefined become the empty string.
console.log("nested=" + String([1, [2, [3]], null, undefined, [null]]));
console.log("emptyArr=" + JSON.stringify(String([])));
console.log("singleNull=" + JSON.stringify(String([null])));
console.log("holes=" + JSON.stringify(String([, , ])));

// Relational comparison uses the NUMBER hint on both sides.
console.log("cmp=" + ([2] < [11]) + " " + (2 < 11));
console.log("cmpStr=" + ([2] < ["11"]) + " " + ("2" < "11"));

// == between an array and a primitive coerces the array; === never does.
console.log("looseEmpty=" + (([] as any) == false) + " strict=" + (([] as any) === false));
console.log("looseOne=" + (([1] as any) == 1) + " looseTwo=" + (([1, 2] as any) == "1,2"));
console.log("looseNull=" + (([] as any) == null) + " looseUndef=" + (([] as any) == undefined));

// toLocaleString delegates per element and is deliberately probed only for the
// shape it must have, not for locale-formatted output.
const t: any = [{ toLocaleString: () => "L1" }, { toLocaleString: () => "L2" }];
console.log("toLocale=" + t.toLocaleString());
console.log("toLocaleNulls=" + JSON.stringify([null, undefined, "x"].toLocaleString()));
