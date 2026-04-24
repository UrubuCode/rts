import { io } from "rts";

function main() {
  let a = 1;
  {
    let a = 2;
    io.print(`inner: ${a}`);
  }
  io.print(`outer: ${a}`);

  let b = 10;
  b = 20;
  io.print(`b = ${b}`);

  const c = 3;
  io.print(`c = ${c}`);

  {
    var v = 7;
  }
  io.print(`v = ${v}`);
}

main();
