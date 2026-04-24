import { io } from "rts";

// Deep self-recursion — without TCO this would overflow the stack.
function loopTco(n: i32): i32 {
  if (n <= 0) {
    return 0;
  }
  return loopTco(n - 1);
}

// Tail accumulator.
function sumTail(n: i32, acc: i32): i32 {
  if (n <= 0) {
    return acc;
  }
  return sumTail(n - 1, acc + n);
}

// Mutual tail recursion.
function isEven(n: i32): i32 {
  if (n <= 0) {
    return 1;
  }
  return isOdd(n - 1);
}

function isOdd(n: i32): i32 {
  if (n <= 0) {
    return 0;
  }
  return isEven(n - 1);
}

io.print(`${loopTco(500000)}`);
io.print(`${sumTail(100, 0)}`);
io.print(`${isEven(10000)}`);
io.print(`${isOdd(10001)}`);
