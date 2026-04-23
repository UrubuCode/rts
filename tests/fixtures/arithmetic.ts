import { io, i32, str } from "rts";

let x: i32 = 10;
let y: i32 = 3;
let sum: i32 = x + y;
let diff: i32 = x - y;
let prod: i32 = x * y;
let quot: i32 = x / y;
let rem: i32 = x % y;

io.print("sum:" + sum);
io.print("diff:" + diff);
io.print("prod:" + prod);
io.print("quot:" + quot);
io.print("rem:" + rem);
