const a = [1, 2, 3, 4, 5];
console.log(a.slice().copyWithin(0, 3).join(","));
console.log(a.slice().copyWithin(0, 3, 4).join(","));
console.log(a.slice().copyWithin(-2).join(","));
console.log(a.slice().copyWithin(-2, -3, -1).join(","));
console.log(a.slice().copyWithin(1, -2, -1).join(","));
console.log([0, 1, 2, 3, 4].copyWithin(2, 0).join(","));
console.log(a.slice().copyWithin(0, 0).join(","));
