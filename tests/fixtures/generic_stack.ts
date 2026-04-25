// Generic class Stack<T> com push/pop.
import { io, gc } from "rts";

class Stack<T> {
  items: T[] = [];

  push(v: T): void {
    const arr = this.items;
    arr.push(v);
  }

  count(): i64 {
    let n: i64 = 0;
    const arr = this.items;
    for (const _v of arr) n = n + 1;
    return n;
  }
}

const s = new Stack<i64>();
s.push(10);
s.push(20);
s.push(30);

const h = gc.string_from_i64(s.count());
io.print(h); gc.string_free(h);
