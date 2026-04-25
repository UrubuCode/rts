// String enum em concat e template.
import { io } from "rts";

enum Logger {
  Info = "[INFO]",
  Warn = "[WARN]",
  Err = "[ERR]",
}

io.print(Logger.Info + " sistema iniciado");
io.print(`${Logger.Warn} memoria baixa`);
const tag: string = Logger.Err;
io.print(tag + " falhou");
