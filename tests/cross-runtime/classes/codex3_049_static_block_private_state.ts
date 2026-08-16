// Cross-runtime: static blocks may read and update private static fields in sequence.
class Registry {
  static #values: number[] = [];
  static first = 1;
  static { this.#values.push(this.first); }
  static second = 2;
  static { this.#values.push(this.second, this.#values[0] + this.second); }
  static dump() { return this.#values.join(","); }
}
console.log(Registry.dump());

