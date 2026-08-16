// ONE thing: which Array methods build their result through ArraySpeciesCreate
// (so a subclass or a Symbol.species override changes the constructor used) and
// which always build a plain Array.
class Sub extends Array {}
const s = Sub.from([1, 2, 3]) as any;

console.log("isSub=" + (s instanceof Sub) + " isArr=" + Array.isArray(s));
console.log("map=" + (s.map((x: number) => x) instanceof Sub));
console.log("filter=" + (s.filter(() => true) instanceof Sub));
console.log("slice=" + (s.slice() instanceof Sub));
console.log("splice=" + (s.slice().splice(0, 1) instanceof Sub));
console.log("concat=" + (s.concat([4]) instanceof Sub));
console.log("flat=" + (s.flat() instanceof Sub));
console.log("flatMap=" + (s.flatMap((x: number) => [x]) instanceof Sub));

// The ES2023 copying methods deliberately do NOT use species — always Array.
console.log("toSorted=" + (s.toSorted() instanceof Sub));
console.log("toReversed=" + (s.toReversed() instanceof Sub));
console.log("toSpliced=" + (s.toSpliced(0, 1) instanceof Sub));
console.log("with=" + (s.with(0, 9) instanceof Sub));

// Neither do the ones that answer a non-array.
console.log("join=" + typeof s.join(","));
console.log("reduceType=" + typeof s.reduce((a: number, b: number) => a + b));

// An explicit species of Array demotes the result.
class Plain extends Array { static get [Symbol.species]() { return Array; } }
const p = Plain.from([1, 2]) as any;
console.log("speciesArray=" + (p.map((x: number) => x) instanceof Plain) + " isArr=" + Array.isArray(p.map((x: number) => x)));

// A species of null or undefined falls back to Array.
const nulled: any = [1, 2];
(nulled as any).constructor = { [Symbol.species]: null };
console.log("nullSpecies=" + Array.isArray(nulled.map((x: number) => x)));

// A species that is not a constructor is a TypeError.
const bad: any = [1, 2];
(bad as any).constructor = { [Symbol.species]: 42 };
try { bad.map((x: number) => x); } catch (e: any) { console.log("badSpecies=" + e.constructor.name); }

// A species constructor is called with the LENGTH for map/filter.
const seen: any[] = [];
class Watch extends Array { constructor(...args: any[]) { seen.push(args.length + ":" + String(args[0])); super(...args); } }
const w = new Watch();
w.push(1, 2, 3);
w.map((x: number) => x);
w.filter(() => true);
console.log("ctorCalls=" + seen.join(" "));

// Static Array.of/from on the subclass use the subclass.
console.log("ofSub=" + (Sub.of(1, 2) instanceof Sub) + " fromSub=" + (Sub.from([1]) instanceof Sub));

// Symbol.species on Array itself is a getter returning this.
console.log("arraySpecies=" + (Array[Symbol.species] === Array));
console.log("subSpecies=" + (Sub[Symbol.species] === Sub));
