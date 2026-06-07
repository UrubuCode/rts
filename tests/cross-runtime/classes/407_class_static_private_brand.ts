// Cross-runtime: private static fields and brand checks.
class Counter {
  static #n = 0;
  #id: number;

  constructor() {
    this.#id = ++Counter.#n;
  }

  static count() {
    return Counter.#n;
  }

  static hasBrand(x: any) {
    return #id in x;
  }

  label() {
    return "C" + this.#id + "/" + Counter.#n;
  }
}

const a = new Counter();
const b = new Counter();
console.log(a.label());
console.log(b.label());
console.log(Counter.count());
console.log(Counter.hasBrand(a) + ":" + Counter.hasBrand({}));
