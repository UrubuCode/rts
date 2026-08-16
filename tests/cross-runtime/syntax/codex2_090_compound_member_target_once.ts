// Cross-runtime: compound assignment evaluates its member target once.
let targetCalls = 0;
let keyCalls = 0;
const o: any = { x: 5 };
const target = () => { targetCalls++; return o; };
const key = () => { keyCalls++; return "x"; };
target()[key()] += 7;
console.log(o.x, targetCalls, keyCalls);

