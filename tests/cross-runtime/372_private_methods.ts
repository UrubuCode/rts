// Cross-runtime: private methods and private accessors.

class LinkedList<T> {
  #head: { val: T; next: any } | null = null;
  #size = 0;

  #makeNode(val: T) { return { val, next: null }; }

  #findLast() {
    let cur = this.#head;
    while (cur?.next) cur = cur.next;
    return cur;
  }

  push(val: T) {
    const node = this.#makeNode(val);
    const last = this.#findLast();
    if (last) last.next = node; else this.#head = node;
    this.#size++;
  }

  pop(): T | undefined {
    if (!this.#head) return undefined;
    if (!this.#head.next) {
      const v = this.#head.val;
      this.#head = null;
      this.#size--;
      return v;
    }
    let prev: any = null, cur: any = this.#head;
    while (cur.next) { prev = cur; cur = cur.next; }
    prev.next = null;
    this.#size--;
    return cur.val;
  }

  get size() { return this.#size; }
  toArray(): T[] {
    const out: T[] = [];
    let cur = this.#head;
    while (cur) { out.push(cur.val); cur = cur.next; }
    return out;
  }
}

const ll = new LinkedList<number>();
ll.push(1); ll.push(2); ll.push(3);
console.log(ll.size);              // 3
console.log(ll.toArray().join(",")); // 1,2,3
console.log(ll.pop());             // 3
console.log(ll.size);              // 2

// private getter/setter
class Temperature {
  #kelvin: number;

  constructor(celsius: number) {
    this.#kelvin = this.#toKelvin(celsius);
  }

  #toKelvin(c: number) { return c + 273.15; }
  #toCelsius(k: number) { return k - 273.15; }

  get celsius() { return this.#toCelsius(this.#kelvin); }
  set celsius(v: number) { this.#kelvin = this.#toKelvin(v); }
  get kelvin() { return this.#kelvin; }
  get fahrenheit() { return this.celsius * 9/5 + 32; }
}

const t = new Temperature(100);
console.log(t.celsius);            // 100
console.log(t.kelvin);             // 373.15
console.log(t.fahrenheit);         // 212
t.celsius = 0;
console.log(t.kelvin);             // 273.15

// private method calling another private method
class Formatter {
  #prefix: string;
  constructor(prefix: string) { this.#prefix = prefix; }

  #pad(s: string, len: number) { return s.padStart(len, " "); }
  #wrap(s: string) { return "[" + this.#prefix + "] " + s; }

  format(msg: string, width = 20): string {
    return this.#pad(this.#wrap(msg), width);
  }
}

const fmt = new Formatter("INFO");
console.log(fmt.format("ok", 15).trimStart()); // [INFO] ok
console.log(fmt.format("x", 20).length);       // 20
