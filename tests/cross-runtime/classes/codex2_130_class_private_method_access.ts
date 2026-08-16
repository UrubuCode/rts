// Cross-runtime: private methods can read private state through the same receiver.
class Calculator {
  #factor = 3;
  #scale(n: number) { return n * this.#factor; }
  run(values: number[]) { return values.map((v) => this.#scale(v)).join(","); }
}
console.log(new Calculator().run([1, 2, 4]));

