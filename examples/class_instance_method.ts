import { io } from "rts";

// Método de instância com `this` e mutação de campo.
class Counter {
  count: number;

  inc(): number {
    this.count = this.count + 1;
    return this.count;
  }
}

function main(): void {
  const c = new Counter();
  c.count = 0;
  c.inc();       // 1
  c.inc();       // 2
  io.print(c.inc()); // 3
}
