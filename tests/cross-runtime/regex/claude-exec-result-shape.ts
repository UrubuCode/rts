// Cross-runtime: the exec result is a REAL Array carrying three extra own
// properties (index, input, groups) — and String.match with /g throws that shape
// away and returns a plain list of strings with no index at all. Pins the whole
// object, including the descriptors of the extras and what JSON does to it.

function own(m: any): string {
  return Object.getOwnPropertyNames(m).join(",");
}

// --- the basic shape ---
const m: any = /(a)(z)?(b)/.exec("xxabyy");
console.log("isArray=" + Array.isArray(m));
console.log("proto=" + (Object.getPrototypeOf(m) === Array.prototype));
console.log("length=" + m.length);
console.log("slots=" + [m[0], m[1], String(m[2]), m[3]].join("|"));
console.log("index=" + m.index);
console.log("input=" + m.input);
console.log("groups=" + String(m.groups));
console.log("own=" + own(m));
console.log("json=" + JSON.stringify(m));
console.log("join=" + m.join("-"));
console.log("map=" + m.map((x: any) => typeof x).join(","));

// --- index/input/groups are ordinary writable, enumerable, configurable props ---
const d: any = Object.getOwnPropertyDescriptor(m, "index");
console.log("desc-index=" + d.writable + "/" + d.enumerable + "/" + d.configurable);
const di: any = Object.getOwnPropertyDescriptor(m, "input");
console.log("desc-input=" + di.writable + "/" + di.enumerable + "/" + di.configurable);
const dg: any = Object.getOwnPropertyDescriptor(m, "groups");
console.log("desc-groups=" + dg.writable + "/" + dg.enumerable + "/" + dg.configurable);
console.log("keys=" + Object.keys(m).join(","));

// --- a non-participating slot is undefined, and JSON turns it into null ---
console.log("nonpart=" + (m[2] === undefined));
console.log("nonpart-in=" + (2 in m));
console.log("nonpart-json=" + JSON.stringify(m[2] === undefined ? null : m[2]));

// --- groups appears (non-undefined) only when the pattern names something ---
const named: any = /(?<w>a)(?<x>z)?/.exec("a");
console.log("named-groups=" + JSON.stringify(named.groups));
console.log("named-groups-keys=" + Object.keys(named.groups).join(","));
console.log("named-own=" + own(named));

// --- index is the offset of the WHOLE match, not of a capture ---
console.log("index-mid=" + (/(b)/.exec("aab") as any).index);
console.log("index-zero=" + (/a/.exec("abc") as any).index);
console.log("index-empty=" + (/(?:)/.exec("abc") as any).index);
console.log("index-lookahead=" + (/(?=b)/.exec("ab") as any).index);

// --- input is the FULL subject, coerced to a string ---
console.log("input-coerced=" + (/2/.exec(123 as any) as any).input);
console.log("input-array=" + (/,/.exec([1, 2] as any) as any).input);
console.log("input-typeof=" + typeof (/a/.exec("a") as any).input);

// --- with /g, exec still gives the full shape; match(/g) does not ---
const g = /(\d)/g;
const e1: any = g.exec("a1b2");
console.log("g-exec=" + e1[0] + "/" + e1[1] + "/" + e1.index + "/" + g.lastIndex);
const list: any = "a1b2".match(/(\d)/g);
console.log("g-match=" + list.join(",") + "/" + list.length);
console.log("g-match-index=" + String(list.index));
console.log("g-match-input=" + String(list.input));
console.log("g-match-groups=" + String(list.groups));
console.log("g-match-own=" + own(list));

// --- without /g, match delegates straight to exec ---
const one: any = "a1b2".match(/(\d)/);
console.log("nong-match=" + one[0] + "/" + one[1] + "/" + one.index + "/" + one.input);
console.log("nong-own=" + own(one));

// --- no match is null for both, never an empty array ---
console.log("exec-null=" + String(/z/.exec("abc")));
console.log("match-null=" + String("abc".match(/z/)));
console.log("match-g-null=" + String("abc".match(/z/g)));

// --- matchAll yields exec-shaped entries ---
const all: any[] = [..."a1b2".matchAll(/(?<d>\d)/g)];
console.log("all-own=" + own(all[0]));
console.log("all-0=" + all[0][0] + "/" + all[0].index + "/" + all[0].groups.d);
console.log("all-1=" + all[1][0] + "/" + all[1].index + "/" + all[1].input);

// --- the d flag adds a fourth extra property ---
const withIndices: any = /(a)(z)?/d.exec("xa");
console.log("d-own=" + own(withIndices));
console.log("d-indices=" + JSON.stringify(withIndices.indices));
console.log("d-indices-isArray=" + Array.isArray(withIndices.indices));
console.log("d-indices-len=" + withIndices.indices.length);
console.log("d-indices-groups=" + String(withIndices.indices.groups));

// --- the result is a fresh array every time ---
const a1 = /a/.exec("a");
const a2 = /a/.exec("a");
console.log("fresh=" + (a1 === a2) + ":" + ((a1 as any)[0] === (a2 as any)[0]));
