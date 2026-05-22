// Cross-runtime: static blocks order and private static access.
class C {
  static order: string[] = [];
  static #v = 3;
  static x = C.order.push("field");
  static { C.order.push("block1:" + C.#v); C.#v += 2; }
  static { C.order.push("block2:" + C.#v); }
}
console.log("order=" + C.order.join("|"));
