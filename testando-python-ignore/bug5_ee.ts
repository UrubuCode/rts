let got = 0;
const ee = new EventEmitter();
ee.on("data", (x: number) => { got = x; });
ee.emit("data", 42);
console.log("closure-listener:"+got);
