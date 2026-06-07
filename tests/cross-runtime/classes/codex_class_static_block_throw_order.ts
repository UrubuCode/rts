// Cross-runtime: static block execution order before class binding is usable.
const log: string[] = [];
try {
  class C {
    static a = log.push("a");
    static {
      log.push("block");
      throw new Error("boom");
    }
    static b = log.push("b");
  }
  console.log(C);
} catch (e: any) {
  console.log(e.message);
}
console.log(log.join(","));
