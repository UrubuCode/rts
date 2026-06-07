// Cross-runtime: TypedArray species affects slice constructor.
class MyU8 extends Uint8Array {
  static get [Symbol.species]() {
    return Uint16Array;
  }
}

const x = new MyU8([1, 2, 3, 4]);
const y = x.slice(1, 3);
console.log(y.constructor.name);
console.log(y.BYTES_PER_ELEMENT);
console.log(Array.from(y).join(","));
