// Cross-runtime compatibility: function metadata and bind/call/apply.
function add(a: number, b: number): number {
  return a + b;
}

const bound = add.bind(null, 5);
console.log("name=" + add.name);
console.log("length=" + add.length);
console.log("call=" + add.call(null, 2, 3));
console.log("apply=" + add.apply(null, [4, 6]));
console.log("bind=" + bound(7));
