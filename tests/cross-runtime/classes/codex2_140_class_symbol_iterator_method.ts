// Cross-runtime: a class can implement iteration with a symbol-named generator method.
class Range {
  start: number;
  end: number;
  constructor(start: number, end: number) {
    this.start = start;
    this.end = end;
  }
  *[Symbol.iterator]() {
    for (let i = this.start; i <= this.end; i++) yield i;
  }
}
const r = new Range(2, 5);
console.log([...r].join(","));
console.log([...r].reduce((a, b) => a + b, 0));
