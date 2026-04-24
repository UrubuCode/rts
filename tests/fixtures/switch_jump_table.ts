import { io } from "rts";

function label(n: i32): void {
  switch (n) {
    case 0: io.print("zero"); break;
    case 1: io.print("one"); break;
    case 2: io.print("two"); break;
    case 5: io.print("five"); break;
    case 10: io.print("ten"); break;
    default: io.print("other");
  }
}

label(0);
label(1);
label(2);
label(3);
label(5);
label(10);
label(99);
label(-1);
