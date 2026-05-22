// Cross-runtime: custom Symbol.iterator (iterable protocol).
class Range {
  constructor(private start: number, private end: number) {}
  [Symbol.iterator]() {
    let cur = this.start;
    const end = this.end;
    return {
      next() {
        if (cur <= end) return { value: cur++, done: false };
        return { value: undefined, done: true };
      }
    };
  }
}

const r = new Range(1, 5);
for (const v of r) console.log(v);

// spread from custom iterable
const vals = [...new Range(10, 13)];
console.log(vals.join(","));

// destructuring from custom iterable
const [a, b, c] = new Range(20, 25);
console.log(a, b, c);

// Array.from custom iterable
const arr = Array.from(new Range(3, 6));
console.log(arr.join(","));
