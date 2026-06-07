// Cross-runtime: defineProperties gathers descriptors before applying them.
const obj: any = {};
const descs: any = {
  get a() {
    console.log("get-a");
    return { value: 1, enumerable: true };
  },
  get b() {
    console.log("get-b");
    throw new Error("bad");
  }
};

try {
  Object.defineProperties(obj, descs);
} catch (e: any) {
  console.log(e.message);
}
console.log(Object.keys(obj).join(","));
