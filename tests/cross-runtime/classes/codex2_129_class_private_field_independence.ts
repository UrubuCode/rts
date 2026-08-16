// Cross-runtime: private fields remain independent per instance.
class Box {
  #value: number;
  constructor(value: number) { this.#value = value; }
  add(n: number) { this.#value += n; return this.#value; }
}
const a = new Box(1);
const b = new Box(10);
console.log(a.add(2), b.add(5), a.add(1));

