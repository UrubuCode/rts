// Cross-runtime: function length stops at the first default and excludes rest.
function a(x: any, y: any, z: any) {}
function b(x: any, y = 1, z: any) {}
function c(x: any, ...rest: any[]) {}
const d = (x: any, y: any = 2) => 0;
console.log([a.length, b.length, c.length, d.length].join(","));

