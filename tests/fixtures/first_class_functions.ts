import { io } from "rts";

function double(x: i32): i32 { return x * 2; }
function triple(x: i32): i32 { return x * 3; }
function addOne(x: i32): i32 { return x + 1; }

// Função que recebe outro funcptr como primeiro argumento e chama.
function apply(fn: i64, x: i32): i32 { return fn(x); }

// Higher-order: compõe dois funcptrs em um valor f(g(x)).
function compose2(f: i64, g: i64, x: i32): i32 {
  return f(g(x));
}

io.print(`${apply(double, 5)}`);
io.print(`${apply(triple, 5)}`);
io.print(`${apply(addOne, 9)}`);
io.print(`${compose2(double, addOne, 4)}`);
