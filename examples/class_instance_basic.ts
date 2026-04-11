import { io } from "rts";

// Exercita `new`, atribuição de campo e leitura de campo.
// Métodos de instância ainda serão feitos no próximo commit
// (precisam de `this` como parâmetro implícito).
class Counter {
  count: number;
}

function main(): void {
  const c = new Counter();
  c.count = 7;
  c.count = c.count + 1;
  io.print(c.count); // espera: 8
}
