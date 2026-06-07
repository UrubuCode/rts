// Cross-runtime: Symbol.toPrimitive precedence in template and arithmetic.
const log: string[] = [];
const obj: any = {
  [Symbol.toPrimitive](hint: string) {
    log.push("prim:" + hint);
    return hint === "number" ? 7 : "S";
  },
  toString() {
    log.push("toString");
    return "T";
  },
  valueOf() {
    log.push("valueOf");
    return 3;
  }
};

console.log(`${obj}`);
console.log(obj + "x");
console.log(+obj);
console.log(log.join("|"));
