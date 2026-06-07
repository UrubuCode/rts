// Cross-runtime: object spread order, getter execution, and overwrite rules.
const log: string[] = [];
const src: any = {
  get a() {
    log.push("get-a");
    return 1;
  },
  b: 2
};
Object.defineProperty(src, "hidden", { enumerable: false, get() { log.push("hidden"); return 9; } });

const out = { z: 0, ...src, a: 3, ...{ c: 4, b: 5 } };
console.log(JSON.stringify(out));
console.log(Object.keys(out).join(","));
console.log(log.join(","));
