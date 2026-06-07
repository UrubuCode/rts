let log: string[] = [];
function side(tag: string, val: any): any { log.push(tag); return val; }
let a = side("A", false) && side("B", true);
console.log(a);
console.log(log.join(","));
let b = side("C", true) || side("D", false);
console.log(b);
console.log(log.join(","));
let n = side("E", null) ?? side("F", 9);
console.log(n);
console.log(log.join(","));
let z = side("G", 0) ?? side("H", 5);
console.log(z);
console.log(log.join(","));