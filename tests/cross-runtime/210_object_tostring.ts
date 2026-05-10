// Cross-runtime: Object.prototype.toString
console.log("obj=" + Object.prototype.toString.call({}));
console.log("arr=" + Object.prototype.toString.call([]));
console.log("num=" + Object.prototype.toString.call(5));
console.log("str=" + Object.prototype.toString.call("hello"));
console.log("bool=" + Object.prototype.toString.call(true));
console.log("null=" + Object.prototype.toString.call(null));
console.log("undef=" + Object.prototype.toString.call(undefined));
console.log("func=" + Object.prototype.toString.call(() => {}));
console.log("date=" + Object.prototype.toString.call(new Date()));
console.log("regex=" + Object.prototype.toString.call(/test/));
console.log("map=" + Object.prototype.toString.call(new Map()));
console.log("set=" + Object.prototype.toString.call(new Set()));
