import { io } from "rts";

function main() {
  // try com throw direto
  try {
    io.print("a");
    throw "first";
  } catch (e) {
    io.print(`caught: ${e}`);
  } finally {
    io.print("finally1");
  }

  // try sem throw
  try {
    io.print("b");
  } catch (e) {
    io.print(`nope: ${e}`);
  }

  // try sem catch (so finally)
  try {
    io.print("c");
  } finally {
    io.print("finally3");
  }

  io.print("end");
}

main();
