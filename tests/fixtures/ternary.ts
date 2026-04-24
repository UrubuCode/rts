import { io } from "rts";

const x = 10;
const label = x > 5 ? "big" : "small";
io.print(label);

const abs = x < 0 ? -x : x;
io.print(`abs = ${abs}`);

const n = 0;
const sign = n > 0 ? "pos" : n < 0 ? "neg" : "zero";
io.print(sign);

function half(v: i32): i32 { return v / 2; }
const y = true ? half(20) : 0;
io.print(`y = ${y}`);
