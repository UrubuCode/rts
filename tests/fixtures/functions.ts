import { io, i32 } from "rts";

function add(a: i32, b: i32): i32 {
    return a + b;
}

function factorial(n: i32): i32 {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

let result: i32 = add(3, 4);
io.print("add:" + result);

let fact5: i32 = factorial(5);
io.print("fact5:" + fact5);
