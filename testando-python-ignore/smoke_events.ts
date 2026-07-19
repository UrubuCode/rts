let got = 0;
function onData(x: number) { got = x; }
const ee = new EventEmitter();
ee.on("data", onData);
ee.emit("data", 42);
console.log("emit:" + got);
console.log("count:" + ee.listenerCount("data"));
