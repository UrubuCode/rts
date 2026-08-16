// Cross-runtime: a regex is an ORDINARY OBJECT with no Symbol.toPrimitive and no
// valueOf of its own, so every coercion goes through the inherited toString —
// which means two identical-looking literals are never equal, a regex is a
// useless Map key, `+/a/` is NaN, and JSON.stringify sees an object with no
// enumerable own properties. Nothing in the corpus treats a regex as a value.

function attempt(f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "!" + e.constructor.name;
  }
}

// --- every literal evaluation makes a NEW object (not true before ES5) ---
function make(): RegExp {
  return /a/g;
}
console.log("literal-identity=" + (make() === make()));
console.log("same-literal=" + ((/a/ as any) === (/a/ as any)));
console.log("ctor-identity=" + (new RegExp("a") === new RegExp("a")));
console.log("string-eq=" + (String(/a/g) === String(/a/g)));
console.log("typeof=" + typeof /a/);
console.log("callable=" + (typeof (/a/ as any).call));

// --- lastIndex is per-object, which is why sharing a literal across calls bites ---
const first = make();
first.lastIndex = 2;
console.log("fresh-lastIndex=" + make().lastIndex + "/" + first.lastIndex);

// --- as a Map / Set member it is compared by identity only ---
const map = new Map<any, number>();
map.set(/a/, 1);
console.log("map-get-new=" + map.get(/a/));
const key = /a/;
map.set(key, 2);
console.log("map-get-same=" + map.get(key));
console.log("map-size=" + map.size);
const set = new Set<any>([/a/, /a/, key, key]);
console.log("set-size=" + set.size);

// --- coercion: no toPrimitive, no own valueOf, so toString does everything ---
console.log("has-toPrimitive=" + ((/a/ as any)[Symbol.toPrimitive] === undefined));
console.log("own-valueOf=" + Object.prototype.hasOwnProperty.call(RegExp.prototype, "valueOf"));
console.log("valueOf-is-object=" + (typeof (/a/ as any).valueOf()));
console.log("plus=" + +(/a/ as any));
console.log("number=" + Number(/12/));
console.log("number-digits=" + Number(new RegExp("12")));
console.log("concat=" + ("x" + /a\/b/gi));
console.log("template=" + `${/a\/b/gi}`);
console.log("string-fn=" + String(/a/y));
console.log("boolean=" + Boolean(new RegExp("")));
console.log("loose-eq-string=" + ((/a/ as any) == "/a/"));
console.log("array-loose-eq=" + (([/a/] as any) == "/a/"));

// --- JSON sees an object with no enumerable own properties ---
console.log("json=" + JSON.stringify(/a\/b/gi));
console.log("json-nested=" + JSON.stringify({ re: /a/g, n: 1 }));
console.log("json-array=" + JSON.stringify([/a/]));
console.log("keys=" + JSON.stringify(Object.keys(/a/g)));
console.log("entries=" + JSON.stringify(Object.entries(/a/g)));
console.log("spread=" + JSON.stringify({ ...(/a/g as any) }));
console.log("assign=" + JSON.stringify(Object.assign({}, /a/g)));

// --- a toJSON added by the user does get used ---
const withToJson: any = /a/g;
withToJson.toJSON = function () {
  return this.source + "|" + this.flags;
};
console.log("json-hook=" + JSON.stringify(withToJson));

// --- the regex is a normal extensible object until frozen ---
const ext: any = /a/;
console.log("extensible=" + Object.isExtensible(ext));
console.log("add-prop=" + Reflect.set(ext, "note", "hi") + "/" + ext.note);
console.log("keys-after=" + JSON.stringify(Object.keys(ext)));
console.log("json-after=" + JSON.stringify(ext));
console.log("sealed=" + Object.isSealed(Object.seal(/a/)));

// --- sorting an array of regexes uses the default ToString comparator ---
const list: any[] = [/b/, /a/g, /a/, /c/i];
console.log("sorted=" + list.sort().map(String).join(" "));
console.log("join=" + [/a/, /b/g].join("+"));

// --- Object.prototype.toString and instanceof ---
console.log("tag=" + Object.prototype.toString.call(/a/));
console.log("toStringTag=" + String((/a/ as any)[Symbol.toStringTag]));
console.log("instanceof-Object=" + (/a/ instanceof Object));
console.log("proto=" + (Object.getPrototypeOf(/a/) === RegExp.prototype));

// --- RegExp called as a function returns its argument when it IS a regex ---
const re = /a/g;
console.log("call-same=" + ((RegExp as any)(re) === re));
console.log("call-new=" + ((new RegExp(re)) === re));
console.log("call-flags=" + (RegExp as any)(re, "i").flags);
console.log("call-string=" + attempt(() => (RegExp as any)("a").source));
