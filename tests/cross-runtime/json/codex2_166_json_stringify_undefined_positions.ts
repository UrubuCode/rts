// Cross-runtime: stringify omits undefined object values but nulls array slots.
const value = {
  a: undefined,
  b: [1, undefined, function () {}, Symbol("x")],
  c: null,
};
console.log(JSON.stringify(value));
console.log(JSON.stringify(undefined), JSON.stringify(function () {}));

