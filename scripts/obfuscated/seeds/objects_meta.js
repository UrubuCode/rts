// Property descriptors, freeze, symbols, enumeration order, JSON round trip.
const out = [];
const o = { b: 2, 2: "two", a: 1, 1: "one" };
out.push(Object.keys(o).join(","));
Object.defineProperty(o, "hidden", { value: 9, enumerable: false });
out.push(Object.keys(o).join(","), o.hidden);
out.push(JSON.stringify(Object.getOwnPropertyDescriptor(o, "hidden")));
// Through `Reflect.set`, which answers `false` in BOTH modes. A plain
// `frozen.v = 2` is silent in sloppy code and throws in strict, and whether an
// obfuscator's output counts as a module is what decides which — so the plain
// write pins the runtime's guess about module-ness rather than the freeze.
// Measured: bun ran it strict and node sloppy, on the same file.
const frozen = Object.freeze({ v: 1 });
out.push(Reflect.set(frozen, "v", 2), frozen.v, Object.isFrozen(frozen));
const sym = Symbol("tag");
const withSym = { [sym]: "s", plain: "p" };
out.push(Object.keys(withSym).join(","), withSym[sym]);
out.push(Object.getOwnPropertySymbols(withSym).length);
class Box { constructor() { this.v = 1; } m() {} }
const bx = new Box();
const seen = [];
for (const k in bx) seen.push(k);
out.push(seen.join(","));
out.push(JSON.stringify(JSON.parse('{"n":[1,{"d":true}],"s":null}')));
out.push(Object.entries({ z: 1, y: 2 }).map(([k, v]) => k + v).join(""));
console.log(out.join("|"));
