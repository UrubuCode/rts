import { io, i32 } from "rts";

let i: i32 = 0;
let acc: i32 = 0;

while (i < 5) {
    acc = acc + i;
    i = i + 1;
}

io.print("acc:" + acc);
