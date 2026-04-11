import { io } from "rts";

class Counter {
  count: number;
  name: string;

  constructor(initCount: number, initName: string) {
    this.count = initCount;
    this.name = initName;
  }
}

function main(): void {
  const c = new Counter(42, "main");
  const s = JSON.stringify(c);
  io.print(s);

  const back = JSON.parse(s);
  io.print(back.count);
  io.print(back.name);
}
