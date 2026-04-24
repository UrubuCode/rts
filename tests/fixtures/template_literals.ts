import { io } from "rts";

io.print(`hello world`);

const name = "RTS";
io.print(`hello ${name}`);

const n = 42;
io.print(`answer is ${n}`);

const pi = 3.14;
io.print(`pi = ${pi}`);

const x = 10;
const y = 20;
io.print(`${x} + ${y} = ${x + y}`);

io.print(`[${name}]`);

const greet = `hi ` + name;
io.print(greet);
