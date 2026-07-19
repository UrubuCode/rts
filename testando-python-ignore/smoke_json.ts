const obj = JSON.parse('{"a":1,"b":[2,3],"c":"hi"}');
console.log("parse-a:" + obj.a);
console.log("parse-c:" + obj.c);
console.log("stringify:" + JSON.stringify({ x: 1, y: [4, 5] }));
const arr = JSON.parse("[10,20,30]");
console.log("arr1:" + arr[1] + ",len=" + arr.length);
// trace exercised via error stack
try { throw new Error("e1"); } catch (e: any) {
  console.log("stack-ok:" + (e.stack.length > 0));
}
