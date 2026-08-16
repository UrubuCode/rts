// Cross-runtime: private static state is shared across class instances.
class Ticket {
  static #next = 1;
  id = Ticket.#next++;
  static current() { return Ticket.#next; }
}
const a = new Ticket();
const b = new Ticket();
console.log(a.id, b.id, Ticket.current());

