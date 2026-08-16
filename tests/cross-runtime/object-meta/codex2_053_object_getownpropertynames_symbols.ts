// Cross-runtime: string and symbol own-key queries remain disjoint.
const s1 = Symbol("one");
const s2 = Symbol.for("two");
const o: any = { a: 1, [s1]: 2 };
Object.defineProperty(o, s2, { value: 3, enumerable: false });
console.log(Object.getOwnPropertyNames(o).join(","));
console.log(Object.getOwnPropertySymbols(o).map(String).join("|"));
console.log(Reflect.ownKeys(o).map(String).join("|"));

