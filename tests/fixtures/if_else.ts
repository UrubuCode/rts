import { io, i32 } from "rts";

let x: i32 = 5;

if (x > 3) {
    io.print("big");
} else {
    io.print("small");
}

if (x === 5) {
    io.print("five");
}
