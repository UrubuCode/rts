import { io } from "rts";

// Counter completo com múltiplos métodos de instância.
// Exercita todo o pedaço 0b: new, this, load/store field, método.
class Counter {
  count: number;

  increment(): number {
    this.count = this.count + 1;
    return this.count;
  }

  decrement(): number {
    this.count = this.count - 1;
    return this.count;
  }

  reset(): number {
    this.count = 0;
    return this.count;
  }
}

function main(): void {
  const c = new Counter();
  c.count = 10;
  c.increment();       // 11
  c.increment();       // 12
  c.decrement();       // 11
  io.print(c.count);   // espera 11

  c.reset();
  io.print(c.count);   // espera 0
}
