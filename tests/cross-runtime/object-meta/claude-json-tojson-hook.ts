const obj = {
  value: 42,
  toJSON() { return { wrapped: this.value, tag: "custom" }; },
};
console.log(JSON.stringify(obj));
console.log(JSON.stringify({ nested: obj, plain: 1 }));
console.log(JSON.stringify([obj, obj]));
const d = new Date(0);
console.log(JSON.stringify({ when: d }));
const num = { toJSON() { return 7; } };
console.log(JSON.stringify(num));
const arr = [1, 2, 3];
arr.toJSON = function () { return "ARR"; };
console.log(JSON.stringify({ a: arr }));
