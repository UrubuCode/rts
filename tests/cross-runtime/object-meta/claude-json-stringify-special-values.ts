console.log(JSON.stringify({ a: undefined, b: function () {}, c: 1 }));
console.log(JSON.stringify([undefined, function () {}, 1]));
console.log(JSON.stringify({ a: NaN, b: Infinity, c: -Infinity }));
console.log(JSON.stringify([NaN, Infinity]));
console.log(JSON.stringify(undefined));
console.log(JSON.stringify(function () {}));
console.log(JSON.stringify(NaN));
console.log(JSON.stringify(null));
console.log(JSON.stringify({ a: null }));
