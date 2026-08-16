// ONE thing: Array.prototype.toString looks up "join" on THIS and calls it when
// callable; otherwise it falls back to Object.prototype.toString.
console.log("plain=" + [1, 2, 3].toString());

const own: any = [1, 2, 3];
own.join = function () { return "OWN"; };
console.log("ownJoin=" + own.toString());
console.log("ownString=" + String(own));
console.log("ownTemplate=" + `${own}`);

const notFn: any = [1, 2, 3];
notFn.join = 42;
console.log("nonCallable=" + notFn.toString());

const fake: any = { join() { return "FAKE"; }, length: 0 };
console.log("fake=" + Array.prototype.toString.call(fake));

const fakeNoJoin: any = { length: 0 };
console.log("fakeNoJoin=" + Array.prototype.toString.call(fakeNoJoin));

class L extends Array { join() { return "SUB"; } }
const s = new L();
s.push(7);
console.log("subclass=" + s.toString());
console.log("subclassLen=" + s.length);
