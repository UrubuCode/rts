// capture_if_enabled retorna 0 quando RUST_BACKTRACE nao esta set.
// Nos testes ele nao esta — checamos comportamento.
import { io, backtrace } from "rts";

const bt = backtrace.capture_if_enabled();
if (bt == 0) {
  io.print("disabled");
} else {
  io.print("enabled");
  backtrace.free(bt);
}
