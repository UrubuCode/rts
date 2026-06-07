const a = [1, [2, [3, [4, [5]]]]];
console.log(JSON.stringify(a.flat()));
console.log(JSON.stringify(a.flat(2)));
console.log(JSON.stringify(a.flat(Infinity)));
console.log(JSON.stringify(a.flat(0)));
const withHoles = [1, , [2, , 3]];
console.log(JSON.stringify(withHoles.flat()));
console.log(JSON.stringify([[1], [2], [3]].flatMap(x => [x[0], x[0] * 10])));
console.log(JSON.stringify([1, 2, 3].flatMap(x => [x, [x]])));
