// ONE thing: how a BigInt turns into text and into a KEY. toString(radix) has
// to work at arbitrary precision, and a BigInt used as a property key becomes
// its decimal string while as a Map key it stays a BigInt.
const n = 255n;
console.log("r2=" + n.toString(2) + " r8=" + n.toString(8) + " r16=" + n.toString(16) + " r36=" + n.toString(36));
console.log("neg=" + (-255n).toString(16) + " zero=" + (0n).toString(16));
console.log("defaultRadix=" + n.toString() + " explicit10=" + n.toString(10));
console.log("undefRadix=" + n.toString(undefined));
console.log("floatRadix=" + n.toString(16.9));
console.log("strRadix=" + n.toString("16" as any));
try { n.toString(1); } catch (e: any) { console.log("radix1=" + e.constructor.name); }
try { n.toString(37); } catch (e: any) { console.log("radix37=" + e.constructor.name); }
try { n.toString(0); } catch (e: any) { console.log("radix0=" + e.constructor.name); }

// Arbitrary precision: a value no double can hold, in three bases.
const big = 2n ** 128n;
console.log("bigDec=" + big.toString());
console.log("bigHex=" + big.toString(16));
console.log("bigBin_len=" + big.toString(2).length);
console.log("bigMinusOneHex=" + (big - 1n).toString(16));
console.log("factorial25=" + [...Array(25)].reduce((a: bigint, _v, i) => a * BigInt(i + 1), 1n).toString());

// String(), template literals and JSON all take different routes.
console.log("String=" + String(42n));
console.log("template=" + `${42n}`);
console.log("concat=" + (42n + ""));
try { JSON.stringify({ v: 1n }); } catch (e: any) { console.log("json=" + e.constructor.name); }
console.log("jsonReplacer=" + JSON.stringify({ v: 1n }, (_k, v) => (typeof v === "bigint" ? v.toString() : v)));

// toLocaleString is deliberately absent here (ICU data differs); valueOf and
// the brand check are not.
console.log("valueOf=" + (1n).valueOf() + " typeof=" + typeof (1n).valueOf());
try { (BigInt.prototype.toString as any).call(1); } catch (e: any) { console.log("wrongBrand=" + e.constructor.name); }
console.log("wrapperBrand=" + BigInt.prototype.toString.call(Object(7n)));

// As a property key it is coerced to the decimal string, so 10n and "10" and
// 10 are all the same property.
const o: any = {};
o[10n as any] = "ten";
console.log("keys=" + Object.keys(o).join(",") + " viaStr=" + o["10"] + " viaNum=" + o[10]);
o[2n ** 70n as any] = "huge";
console.log("hugeKey=" + Object.keys(o).join("|"));

// As a Map key it stays a BigInt and never equals the Number.
const m = new Map<any, string>();
m.set(1n, "big"); m.set(1, "num");
console.log("mapSize=" + m.size + " big=" + m.get(1n) + " num=" + m.get(1));
console.log("setDedup=" + new Set([1n, 1n, 1, 1]).size);

// Sorting BigInts with the default comparator uses their STRING form.
console.log("defaultSort=" + [10n, 9n, 100n, 2n].sort().join(","));
console.log("numericSort=" + [10n, 9n, 100n, 2n].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join(","));
