// Cross-runtime: Map.forEach uses its thisArg and passes value before key.
const m = new Map([["x", 2], ["y", 3]]);
const ctx = { sum: 0, text: "" };
m.forEach(function (this: typeof ctx, value, key) {
  this.sum += value;
  this.text += key;
}, ctx);
console.log(ctx.sum, ctx.text);

