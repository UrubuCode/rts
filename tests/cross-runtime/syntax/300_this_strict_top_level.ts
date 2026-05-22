// Cross-runtime: strict vs sloppy this probes.
console.log("strict=" + String((function () { "use strict"; return this === undefined; })()));
console.log("sloppy=" + String(Function("return this === globalThis")()));
