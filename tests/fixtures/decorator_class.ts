// Class decorator: executa como side-effect na declaracao.
import { io } from "rts";

function register(target: i64): i64 {
  io.print("classe registrada");
  return target;
}

@register
class Service {
  greet(): void {
    io.print("oi do service");
  }
}

new Service().greet();
