// Cross-runtime: private accessors compose with compound assignment exactly once.
class Counter {
  #cell = 2;
  get #value() { return this.#cell; }
  set #value(v: number) { this.#cell = v * 2; }
  update() { return this.#value += 3; }
  read() { return this.#cell; }
}
const c = new Counter();
console.log(c.update(), c.read());

