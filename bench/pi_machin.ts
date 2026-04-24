import { io, math } from "rts";

// Machin's formula: pi = 16 * atan(1/5) - 4 * atan(1/239)
// Precisao de maquina (double) em uma unica expressao.
const pi: f64 = 16.0 * math.atan(1.0 / 5.0) - 4.0 * math.atan(1.0 / 239.0);

io.print(`pi (Machin)  = ${pi}`);
io.print(`pi (real)    = ${math.PI}`);
io.print(`erro absoluto= ${math.abs_f64(pi - math.PI)}`);
