// Cross-runtime: Symbol.toPrimitive receives distinct hints from coercion sites.
const seen: string[] = [];
const value = {
  [Symbol.toPrimitive](hint: string) {
    seen.push(hint);
    return hint === "number" ? 4 : hint === "string" ? "S" : "D";
  },
};
console.log(+value, String(value), value + "!");
console.log(`${value}`, value == "D");
console.log(seen.join(","));

