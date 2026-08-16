// Cross-runtime: rebinding cannot replace an already bound receiver but adds arguments.
function show(this: any, ...args: any[]) { return this.name + ":" + args.join(","); }
const first = show.bind({ name: "first" }, "a");
const second = first.bind({ name: "second" }, "b");
console.log(second("c"));
console.log(second.call({ name: "third" }, "d"));

