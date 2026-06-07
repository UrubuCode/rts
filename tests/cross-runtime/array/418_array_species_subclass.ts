// Cross-runtime: Array subclass species affects map/filter constructors.
class PlainArray<T> extends Array<T> {
  static get [Symbol.species]() {
    return Array;
  }
}

class FancyArray<T> extends Array<T> {}

const plain = new PlainArray(1, 2, 3).map(x => x * 2);
const fancy = new FancyArray(1, 2, 3).filter(x => x > 1);
console.log(Array.isArray(plain) + ":" + (plain instanceof PlainArray) + ":" + plain.join(","));
console.log(Array.isArray(fancy) + ":" + (fancy instanceof FancyArray) + ":" + fancy.join(","));
