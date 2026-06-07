// Cross-runtime: String.raw accepts array-like raw payload.
const callSite: any = { raw: { 0: "a", 1: "b", 2: "c", length: 3 } };
console.log(String.raw(callSite, 1, 2));
console.log(String.raw({ raw: ["x\n", "y"] }, "MID"));
