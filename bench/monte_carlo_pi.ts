import { io, math } from "rts";

// Monte Carlo estimation of pi:
// throw N points into the unit square [0,1)x[0,1); count how many land
// inside the quarter circle x^2 + y^2 <= 1; pi ~ 4 * inside / N.

const N = 10_000_000;
math.seed(1);

let inside = 0;
let i = 0;
while (i < N) {
  const x = math.random();
  const y = math.random();
  if (x * x + y * y <= 1.0) {
    inside = inside + 1;
  }
  i = i + 1;
}

// Force the division into float land by routing through sqrt: sqrt(x)^2 = x
// but the intermediate value is f64, which poisons the rest of the expression.
function toFloat(x: i32): f64 {
  return math.sqrt(x) * math.sqrt(x);
}

const pi: f64 = 4.0 * toFloat(inside) / toFloat(N);
io.print(`N      = ${N}`);
io.print(`inside = ${inside}`);
io.print(`pi     = ${pi}`);
