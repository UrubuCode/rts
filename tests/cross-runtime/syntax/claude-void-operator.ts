console.log(void 0);
console.log(void "hello");
console.log(void (1 + 2));
let log: string[] = [];
function f(): number { log.push("called"); return 99; }
let r = void f();
console.log(r);
console.log(log.join(","));
console.log(void 0 === undefined);
let x = 5;
console.log(void x++);
console.log(x);
let arr = [void 1, void 2];
console.log(arr.length);
console.log(typeof void 0);