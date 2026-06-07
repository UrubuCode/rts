// Cross-runtime: array length truncation and defineProperty index growth.
const a: any[] = [1, 2, 3, 4];
Object.defineProperty(a, "6", { value: 7, enumerable: true, configurable: true, writable: true });
console.log("after-define=" + a.length + ":" + a.join("|") + ":" + Object.keys(a).join(","));

a.length = 2;
console.log("after-trunc=" + a.length + ":" + a.join("|") + ":" + Object.keys(a).join(","));

try {
  Object.defineProperty(a, "length", { value: 1.5 });
} catch (e: any) {
  console.log(e.constructor.name);
}
console.log("final=" + a.length + ":" + a.join("|"));
