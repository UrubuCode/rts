import { io } from "rts";

// Exercita operador compound e update (++) sobre field.
class Tally {
  value: number;
}

function main(): void {
  const t = new Tally();
  t.value = 0;
  t.value += 10;    // 10
  t.value++;        // 11
  t.value *= 2;     // 22
  io.print(t.value); // espera: 22
}
