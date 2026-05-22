// Cross-runtime: Symbol.hasInstance, Symbol.isConcatSpreadable, Symbol.species.

// Symbol.hasInstance — custom instanceof
class EvenNumber {
  static [Symbol.hasInstance](v: unknown): boolean {
    return typeof v === "number" && v % 2 === 0;
  }
}
console.log(2 instanceof EvenNumber);   // true
console.log(3 instanceof EvenNumber);   // false
console.log(4 instanceof EvenNumber);   // true
console.log("2" instanceof EvenNumber); // false

// Symbol.isConcatSpreadable
const arr = [1, 2, 3];
const spreadable = { 0: 4, 1: 5, length: 2, [Symbol.isConcatSpreadable]: true };
const notSpreadable = [6, 7];
(notSpreadable as any)[Symbol.isConcatSpreadable] = false;

console.log([0].concat(arr as any).join(","));           // 0,1,2,3
console.log([0].concat(spreadable as any).join(","));    // 0,4,5
console.log([0].concat(notSpreadable as any).join(","));  // 0,6,7 (as single item)

// Symbol.species — controls constructor used in derived classes
class PowerArray extends Array<number> {
  static get [Symbol.species]() { return Array; }
  sum() { return this.reduce((a, b) => a + b, 0); }
}
const pa = new PowerArray(1, 2, 3, 4);
const mapped = pa.map(x => x * 2);
// mapped is plain Array (due to Symbol.species), not PowerArray
console.log(mapped instanceof Array);       // true
console.log(mapped instanceof PowerArray);  // false (species is Array)
console.log(mapped.join(","));              // 2,4,6,8

// Symbol.toPrimitive (already in 350, but combined with hasInstance)
class Meter {
  #value: number;
  constructor(v: number) { this.#value = v; }
  [Symbol.toPrimitive](hint: string) {
    if (hint === "number") return this.#value;
    if (hint === "string") return this.#value + "m";
    return this.#value;
  }
  static [Symbol.hasInstance](v: any) {
    return typeof v === "object" && v !== null && Symbol.toPrimitive in v;
  }
}
const m = new Meter(5);
console.log(+m);      // 5
console.log(`${m}`);  // 5m
console.log(m instanceof Meter);  // true (own instance)
