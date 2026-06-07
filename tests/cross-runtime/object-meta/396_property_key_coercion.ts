// Cross-runtime: property key coercion order and collisions.
const log: string[] = [];
const keyObj = {
  toString() {
    log.push("toString");
    return "7";
  },
  valueOf() {
    log.push("valueOf");
    return 8;
  }
};

const obj: any = { 7: "seven", true: "bool", "[object Object]": "plain" };
console.log(obj[keyObj as any]);
console.log(log.join(","));
console.log(obj[true as any]);
console.log(obj[{} as any]);

const arr: any[] = ["zero", "one", "two"];
arr[keyObj as any] = "seven-index";
console.log(arr.length + ":" + arr[7] + ":" + Object.keys(arr).join("|"));
