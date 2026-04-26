// backtrace.capture + to_string: nao podemos comparar conteudo
// (varia por build/OS), so validamos que capture retorna handle
// nao-zero e que to_string produz string GC.
import { io, gc, backtrace } from "rts";

const bt = backtrace.capture();
if (bt == 0) {
  io.print("FAIL: capture retornou 0");
} else {
  io.print("captured");
}

const s = backtrace.to_string(bt);
if (s == 0) {
  io.print("FAIL: to_string retornou 0");
} else {
  io.print("formatted");
  gc.string_free(s);
}

backtrace.free(bt);
io.print("freed");
