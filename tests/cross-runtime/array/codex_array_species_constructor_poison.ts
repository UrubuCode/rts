// Cross-runtime: Array species constructor getter is consulted by derived methods.
class Weird<T> extends Array<T> {
  static get [Symbol.species]() {
    return function (len: number) {
      const out: any[] = [];
      out.createdLength = len;
      return out;
    } as any;
  }
}

const w = new Weird(1, 2, 3);
const mapped: any = w.map(x => x * 10);
console.log(Array.isArray(mapped));
console.log(mapped.join(","));
console.log(mapped.createdLength);
