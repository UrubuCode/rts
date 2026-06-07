// Cross-runtime: toJSON runs before replacer, including nested objects.
const log: string[] = [];
const obj: any = {
  a: 1,
  child: {
    toJSON(key: string) {
      log.push("toJSON:" + key);
      return { b: 2, c: 3 };
    }
  }
};

const out = JSON.stringify(obj, (key, value) => {
  log.push("rep:" + key + ":" + (value && value.b ? "child" : typeof value));
  if (key === "c") return undefined;
  return value;
});

console.log(out);
console.log(log.join("|"));
