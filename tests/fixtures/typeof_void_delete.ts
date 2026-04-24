import { io } from "rts";

function main() {
  const n: i32 = 42;
  const s: string = "hello";
  const b: boolean = true;

  io.print(typeof n);
  io.print(typeof s);
  io.print(typeof b);

  // void avalia e descarta — o retorno (0) nao deve surgir no fluxo
  // alem do que atribuirmos.
  const voided = void 999;
  io.print(`voided = ${voided}`);

  // delete em variavel non-property e no-op; retorna true.
  const d = delete n;
  io.print(`delete = ${d}`);
}

main();
