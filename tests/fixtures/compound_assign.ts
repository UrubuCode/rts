import { io } from "rts";

function main() {
  let x: i32 = 10;
  x += 5;
  io.print(`${x}`);
  x -= 3;
  io.print(`${x}`);
  x *= 2;
  io.print(`${x}`);
  x /= 4;
  io.print(`${x}`);
  x %= 4;
  io.print(`${x}`);

  let bits: i32 = 0xF0;
  bits &= 0x3C;
  io.print(`${bits}`);
  bits |= 0x03;
  io.print(`${bits}`);
  bits ^= 0x33;
  io.print(`${bits}`);

  let shift: i32 = 1;
  shift <<= 4;
  io.print(`${shift}`);
  shift >>= 2;
  io.print(`${shift}`);
}

main();
