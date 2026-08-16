// Cross-runtime: function names are inferred from binding and property positions.
const alpha = function () {};
const obj = { beta() {}, gamma: function () {}, delta: () => {} };
console.log([alpha.name, obj.beta.name, obj.gamma.name, obj.delta.name].join("|"));

