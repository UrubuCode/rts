// Cross-runtime: serialized async class updates retain private state across promise reactions.
class Queue {
  #value = 0;
  #tail = Promise.resolve();
  add(delta: number) {
    this.#tail = this.#tail.then(() => { this.#value += delta; });
    return this.#tail.then(() => this.#value);
  }
  read() { return this.#tail.then(() => this.#value); }
}
const queue = new Queue();
Promise.all([queue.add(2), queue.add(3), queue.add(-1), queue.read()])
  .then((values) => console.log(values.join(",")));

