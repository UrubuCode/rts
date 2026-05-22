// Cross-runtime: private static fields + methods + static initializer blocks.

class Registry {
  static #instances = new Map<string, Registry>();
  static #count = 0;

  readonly #name: string;
  readonly #id: number;

  private constructor(name: string) {
    this.#name = name;
    this.#id = ++Registry.#count;
  }

  static get(name: string): Registry {
    if (!Registry.#instances.has(name)) {
      Registry.#instances.set(name, new Registry(name));
    }
    return Registry.#instances.get(name)!;
  }

  static #validate(name: string): boolean {
    return typeof name === "string" && name.length > 0;
  }

  static create(name: string): Registry {
    if (!Registry.#validate(name)) throw new TypeError("invalid name");
    return Registry.get(name);
  }

  static get totalCreated() { return Registry.#count; }

  toString() { return "Registry(" + this.#name + "#" + this.#id + ")"; }
}

const r1 = Registry.create("alpha");
const r2 = Registry.create("beta");
const r3 = Registry.get("alpha");  // same instance

console.log(r1.toString());          // Registry(alpha#1)
console.log(r2.toString());          // Registry(beta#2)
console.log(r1 === r3);              // true
console.log(Registry.totalCreated);  // 2

try {
  Registry.create("");
} catch (e: any) {
  console.log(e.message);            // invalid name
}

// Static initializer block (ES2022)
class Config {
  static readonly DEBUG: boolean;
  static readonly VERSION: string;
  static readonly LIMITS: { min: number; max: number };

  static {
    Config.DEBUG = false;
    Config.VERSION = "2.0.0";
    Config.LIMITS = { min: 0, max: 100 };
  }
}

console.log(Config.DEBUG);          // false
console.log(Config.VERSION);        // 2.0.0
console.log(Config.LIMITS.min);     // 0
console.log(Config.LIMITS.max);     // 100

// Static block with complex initialization
class LookupTable {
  static #table: Map<number, string>;
  static #reverse: Map<string, number>;

  static {
    const entries: [number, string][] = [
      [1, "one"], [2, "two"], [3, "three"], [4, "four"], [5, "five"]
    ];
    LookupTable.#table = new Map(entries);
    LookupTable.#reverse = new Map(entries.map(([k, v]) => [v, k]));
  }

  static toWord(n: number) { return LookupTable.#table.get(n) ?? "unknown"; }
  static toNum(s: string)  { return LookupTable.#reverse.get(s) ?? -1; }
}

console.log(LookupTable.toWord(3));   // three
console.log(LookupTable.toNum("five")); // 5
console.log(LookupTable.toWord(99));  // unknown
