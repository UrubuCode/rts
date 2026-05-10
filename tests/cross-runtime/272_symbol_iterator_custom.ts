// Cross-runtime: custom Symbol.iterator with for...of.
class Bag {
  data = [3, 4, 5];
  [Symbol.iterator]() {
    let i = 0;
    const d = this.data;
    return { next() { return i < d.length ? { value: d[i++], done: false } : { value: undefined, done: true }; } };
  }
}
const out: number[] = [];
for (const n of new Bag() as any) out.push(n);
console.log("iter=" + out.join(","));
