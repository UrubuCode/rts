// Cross-runtime: computed property names in classes.

const PREFIX = "get";
const SUFFIX = "Value";

class Config {
  #data: Record<string, number> = { width: 100, height: 200, depth: 50 };

  [PREFIX + "Width" + SUFFIX]()  { return this.#data.width; }
  [PREFIX + "Height" + SUFFIX]() { return this.#data.height; }
  [PREFIX + "Depth" + SUFFIX]()  { return this.#data.depth; }

  [Symbol.toPrimitive](hint: string) {
    if (hint === "string") return JSON.stringify(this.#data);
    return Object.values(this.#data).reduce((a, b) => a + b, 0);
  }

  [(1 + 1).toString()]() { return "method named '2'"; }
}

const cfg = new Config();
console.log(cfg.getWidthValue());   // 100
console.log(cfg.getHeightValue());  // 200
console.log(+cfg);                  // 350
console.log((cfg as any)[2]());     // method named '2'

// Dynamic method names via Object.assign to prototype
const ops = ["add", "sub", "mul"] as const;
const methods: Record<string, (a: number, b: number) => number> = {
  add: (a, b) => a + b,
  sub: (a, b) => a - b,
  mul: (a, b) => a * b,
};
class Calculator {}
ops.forEach(op => {
  (Calculator.prototype as any)[op] = methods[op];
});
const calc = new Calculator() as any;
console.log(calc.add(3, 4));  // 7
console.log(calc.sub(10, 3)); // 7
console.log(calc.mul(3, 4));  // 12

// Computed keys with Symbols
const kSerialize = Symbol("serialize");
const kDeserialize = Symbol("deserialize");

class Payload {
  constructor(public data: any) {}
  [kSerialize]() { return JSON.stringify(this.data); }
  static [kDeserialize](s: string) { return new Payload(JSON.parse(s)); }
}

const p = new Payload({ x: 1 });
const serialized = p[kSerialize]();
console.log(serialized);  // {"x":1}
const restored = Payload[kDeserialize](serialized);
console.log(restored.data.x);  // 1
