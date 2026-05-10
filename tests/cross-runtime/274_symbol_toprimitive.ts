// Cross-runtime: Symbol.toPrimitive coercion hints.
const obj = {
  [Symbol.toPrimitive](hint: string) {
    if (hint === "number") return 7;
    if (hint === "string") return "str!";
    return "def!";
  },
};
console.log("num=" + (+obj));
console.log("str=" + String(obj));
console.log("def=" + ((obj as any) + ""));
