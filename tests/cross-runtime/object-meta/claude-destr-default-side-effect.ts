let order = [];
function d(tag, val) { order.push(tag); return val; }
const obj = { x: 10, z: undefined };
const { x = d("x", -1), y = d("y", -2), z = d("z", -3) } = obj;
console.log(x);
console.log(y);
console.log(z);
console.log(order.join(","));
const arr = [5, undefined];
const [p = d("p", 100), q = d("q", 200), r = d("r", 300)] = arr;
console.log(p, q, r);
console.log(order.join(","));
